//! Editor-side syntax highlighting glue.
//!
//! The structural parsing + provider trait live in `bevy_tree_sitter`. This
//! module owns the editor-only pieces:
//!
//! - [`SyntaxResource`]: a thin newtype around `Arc<RwLock<SyntaxInner>>`
//!   that holds an `Option<TreeSitterProvider>` plus a `tree_version`
//!   counter the renderer watches for invalidation. The Arc is shared with
//!   the editor's `LineStyleSource` Component so the styling system reads
//!   from the same state the parse pipeline writes to.
//! - [`HighlightCache`]: LRU of structural `HighlightRange` runs per line,
//!   version-keyed by `content_version`. Theme mapping happens on the read
//!   path so theme hot-swap doesn't need a cache flush.
//! - The async-parse pipeline: forwards edits to the tree (sync interpolation)
//!   and consumes [`bevy_tree_sitter::ParseCompleted`] events to keep the
//!   provider's tree fresh.

use crate::types::LineSegment;
#[cfg(feature = "tree-sitter")]
use crate::text_view::TextViewState;
#[cfg(feature = "tree-sitter")]
use crate::types::{CodeEditor, SyntaxCacheState};
use bevy::prelude::*;
use std::collections::VecDeque;
use std::sync::{Arc, RwLock};

#[cfg(feature = "tree-sitter")]
use bevy_tree_sitter::{HighlightRange, ParseCompleted, SyntaxProvider, TreeSitterProvider};

/// Mutable state inside [`SyntaxResource`]. Held behind an `Arc<RwLock<_>>`
/// so the editor's parse-pipeline systems and the engine's `LineStyleSource`
/// can share access without each owning their own copy.
pub struct SyntaxInner {
    #[cfg(feature = "tree-sitter")]
    pub(crate) provider: Option<TreeSitterProvider>,
    #[cfg(feature = "tree-sitter")]
    pub(crate) tree_version: u64,
}

impl SyntaxInner {
    fn new() -> Self {
        Self {
            #[cfg(feature = "tree-sitter")]
            provider: None,
            #[cfg(feature = "tree-sitter")]
            tree_version: 0,
        }
    }
}

/// Resource that holds the syntax highlighting provider.
///
/// Public methods preserve their pre-Arc signatures — locking happens
/// internally. The wrapping `Arc<RwLock<_>>` is exposed via [`share_arc`]
/// for components (e.g. `LineStyleSource`) that need to read from the same
/// state.
// not reflectable: holds `TreeSitterProvider` which wraps `tree_sitter::*`
// types that don't implement `Reflect`.
#[derive(Resource, Clone)]
pub struct SyntaxResource {
    pub(crate) inner: Arc<RwLock<SyntaxInner>>,
}

impl SyntaxResource {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(SyntaxInner::new())),
        }
    }

    /// Hand out a clone of the inner `Arc<RwLock<_>>`. Cheap (refcount bump).
    /// Used by the editor's `LineStyleSource` to share state with the
    /// engine's `produce_layouts` system.
    pub fn share_arc(&self) -> Arc<RwLock<SyntaxInner>> {
        self.inner.clone()
    }

    #[cfg(feature = "tree-sitter")]
    pub fn set_provider(&mut self, provider: TreeSitterProvider) {
        self.inner.write().unwrap().provider = Some(provider);
    }

    /// Bumped each time the parse tree is replaced. The display-map
    /// fingerprint folds this in so a finished parse triggers a re-layout.
    #[cfg(feature = "tree-sitter")]
    pub fn tree_version(&self) -> u64 {
        self.inner.read().unwrap().tree_version
    }

    /// Run a closure with `&Tree` access, if a tree is cached. Returns
    /// `None` if no provider / no tree.
    #[cfg(feature = "tree-sitter")]
    pub fn with_tree<R>(
        &self,
        f: impl FnOnce(&bevy_tree_sitter::ts::Tree) -> R,
    ) -> Option<R> {
        let guard = self.inner.read().unwrap();
        guard.provider.as_ref()?.tree().map(f)
    }

    pub fn is_available(&self) -> bool {
        #[cfg(feature = "tree-sitter")]
        {
            self.inner
                .read()
                .unwrap()
                .provider
                .as_ref()
                .map(|p| p.is_available())
                .unwrap_or(false)
        }

        #[cfg(not(feature = "tree-sitter"))]
        {
            false
        }
    }

    /// Highlight a line range and return styled segments — the editor's
    /// renderer-facing entry point.
    ///
    /// Internally: ask the provider for structural `HighlightRange`s, then
    /// map each capture name through `map_highlight_color`. We re-do the
    /// color mapping every call (sub-microsecond per range) so theme changes
    /// don't need cache invalidation.
    #[cfg(feature = "tree-sitter")]
    pub fn highlight_range(
        &mut self,
        text: &str,
        start_line: usize,
        end_line: usize,
        start_byte: usize,
        theme: &crate::settings::SyntaxTheme,
        default_color: Color,
    ) -> Vec<Vec<LineSegment>> {
        let mut guard = self.inner.write().unwrap();
        let Some(provider) = &mut guard.provider else {
            return plain_text_segments(text, default_color);
        };
        let highlights = provider.highlight_range(text, start_line, end_line, start_byte);
        ranges_to_segments(text, start_byte, &highlights, theme, default_color)
    }

    /// No-tree-sitter fallback — emit a single default-colored segment per line.
    /// Keeps callers in `display_map::layout` and `input::editing` portable
    /// when the `tree-sitter` feature is off.
    #[cfg(not(feature = "tree-sitter"))]
    pub fn highlight_range(
        &mut self,
        text: &str,
        _start_line: usize,
        _end_line: usize,
        _start_byte: usize,
        _theme: &crate::settings::SyntaxTheme,
        default_color: Color,
    ) -> Vec<Vec<LineSegment>> {
        plain_text_segments(text, default_color)
    }

    /// Drop the cached parse tree (e.g. when content shifts catastrophically).
    #[cfg(feature = "tree-sitter")]
    pub fn invalidate_tree(&mut self) {
        if let Some(provider) = &mut self.inner.write().unwrap().provider {
            provider.invalidate_tree();
        }
    }

    /// Re-parse the tree from `rope` synchronously.
    #[cfg(feature = "tree-sitter")]
    pub fn update_tree(&mut self, rope: &ropey::Rope) {
        if let Some(provider) = &mut self.inner.write().unwrap().provider {
            provider.update_tree(rope);
        }
    }

    /// Snapshot the parse state for an off-thread re-parse.
    ///
    /// Edits should already have been applied to the cached tree via
    /// `apply_sync_edit()`; the cloned tree is ready for incremental parse.
    #[cfg(feature = "tree-sitter")]
    pub fn clone_parse_state(
        &mut self,
    ) -> (
        Option<bevy_tree_sitter::ts::Parser>,
        Option<bevy_tree_sitter::ts::Language>,
        Option<bevy_tree_sitter::ts::Tree>,
    ) {
        let mut guard = self.inner.write().unwrap();
        if let Some(provider) = &mut guard.provider {
            let parser = if let Some(ref language) = provider.cached_language {
                let mut new_parser = bevy_tree_sitter::ts::Parser::new();
                if new_parser.set_language(language).is_ok() {
                    Some(new_parser)
                } else {
                    None
                }
            } else {
                None
            };

            let tree = provider.cached_tree.clone();

            (parser, provider.cached_language.clone(), tree)
        } else {
            (None, None, None)
        }
    }

    /// Install a freshly-parsed tree (and the rope it parsed).
    /// Bumps `tree_version` so the display-map plugin re-runs.
    #[cfg(feature = "tree-sitter")]
    pub fn set_parsed_tree(&mut self, tree: bevy_tree_sitter::ts::Tree, rope: &ropey::Rope) {
        let mut guard = self.inner.write().unwrap();
        let inner = &mut *guard;
        if let Some(provider) = &mut inner.provider {
            provider.cached_tree = Some(tree);
            provider.cached_rope = Some(rope.clone());
            if provider.cached_parser.is_none() {
                if let Some(ref language) = provider.cached_language {
                    let mut parser = bevy_tree_sitter::ts::Parser::new();
                    if parser.set_language(language).is_ok() {
                        provider.cached_parser = Some(parser);
                    }
                }
            }

            inner.tree_version += 1;
        }
    }

    /// Apply an edit synchronously (tree interpolation). Keeps the tree
    /// valid for highlight queries while the async re-parse runs.
    #[cfg(feature = "tree-sitter")]
    pub fn apply_sync_edit(&mut self, edit: bevy_tree_sitter::ts::InputEdit, rope: &ropey::Rope) {
        if let Some(provider) = &mut self.inner.write().unwrap().provider {
            provider.apply_sync_edit(edit, rope);
        }
    }
}

impl Default for SyntaxResource {
    fn default() -> Self {
        Self::new()
    }
}

/// Build LineSegments for `text` with no highlights — fallback for when no
/// provider is installed.
fn plain_text_segments(text: &str, default_color: Color) -> Vec<Vec<LineSegment>> {
    text.lines()
        .map(|line| {
            if line.trim().is_empty() {
                vec![]
            } else {
                vec![LineSegment {
                    text: line.to_string(),
                    color: default_color,
                    background: None,
                    corner_radius: 0.0,
                    font_scale: 0.0,
                    skew: 0.0,
                }]
            }
        })
        .collect()
}

/// Translate per-line `HighlightRange`s back into `LineSegment`s, mapping
/// capture names through the editor's `SyntaxTheme`.
///
/// `start_byte` is the document byte offset of `text` — `HighlightRange`
/// byte ranges are document-absolute, so we subtract `start_byte` to index
/// into `text`.
#[cfg(feature = "tree-sitter")]
fn ranges_to_segments(
    text: &str,
    start_byte: usize,
    per_line: &[Vec<HighlightRange>],
    theme: &crate::settings::SyntaxTheme,
    default_color: Color,
) -> Vec<Vec<LineSegment>> {
    let mut out: Vec<Vec<LineSegment>> = Vec::with_capacity(per_line.len().max(text.lines().count()));
    let mut byte_pos = 0usize;

    for (line, ranges) in text.lines().zip(per_line.iter()) {
        let line_start = byte_pos;
        let line_len = line.len();
        let line_end = line_start + line_len;

        // Walk the line in document bytes, slicing out segments where each
        // range starts/ends. Gaps between ranges become default-colored
        // segments. Ranges land sorted from the provider.
        let mut segments: Vec<LineSegment> = Vec::with_capacity(ranges.len() + 1);
        let mut cursor = line_start;
        for range in ranges {
            let range_start = range.byte_range.start.max(line_start).min(line_end);
            let range_end = range.byte_range.end.max(line_start).min(line_end);

            if range_start > cursor {
                let local_lo = cursor - line_start;
                let local_hi = range_start - line_start;
                let slice = &line[local_lo..local_hi];
                if !slice.is_empty() {
                    segments.push(LineSegment {
                        text: slice.to_string(),
                        color: default_color,
                        background: None,
                        corner_radius: 0.0,
                        font_scale: 0.0,
                        skew: 0.0,
                    });
                }
                cursor = range_start;
            }

            if range_end > cursor {
                let local_lo = cursor - line_start;
                let local_hi = range_end - line_start;
                let slice = &line[local_lo..local_hi];
                if !slice.is_empty() {
                    let color = crate::syntax::map_highlight_color(
                        Some(&range.capture_name),
                        theme,
                        default_color,
                    );
                    segments.push(LineSegment {
                        text: slice.to_string(),
                        color,
                        background: None,
                        corner_radius: 0.0,
                        font_scale: 0.0,
                        skew: 0.0,
                    });
                }
                cursor = range_end;
            }
        }

        if cursor < line_end {
            let local_lo = cursor - line_start;
            let slice = &line[local_lo..];
            if !slice.is_empty() {
                segments.push(LineSegment {
                    text: slice.to_string(),
                    color: default_color,
                    background: None,
                    corner_radius: 0.0,
                    font_scale: 0.0,
                    skew: 0.0,
                });
            }
        }

        // Suppress whitespace-only lines (the renderer treats empty Vec as
        // "no segments" and falls back to default styling without paying for
        // a glyph run).
        if segments.iter().all(|s| s.text.trim().is_empty()) {
            out.push(Vec::new());
        } else {
            out.push(segments);
        }

        byte_pos = line_end + 1;
    }

    let _ = start_byte; // currently unused after switching to absolute ranges

    // Pad with empty rows if the provider returned fewer than expected.
    while out.len() < per_line.len() {
        out.push(Vec::new());
    }
    out
}

// ========== Highlight Cache ==========

/// A cached range of highlighted lines
/// NOTE: We only cache by content_version, not tree_version.
/// When tree updates, stale entity detection handles re-highlighting.
#[derive(Clone)]
struct CachedRange {
    start_line: usize,
    end_line: usize,
    content_version: u64,
    lines: Vec<Vec<LineSegment>>,
}

// not reflectable: `CachedRange` (private) holds `Vec<Vec<LineSegment>>`
// and the cache is hot-path internal state, not user-facing data.
#[derive(Resource)]
pub struct HighlightCache {
    ranges: VecDeque<CachedRange>,
    max_ranges: usize,
    pub last_highlight_time: f64,
    pub debounce_ms: f64,
}

impl Default for HighlightCache {
    fn default() -> Self {
        Self {
            ranges: VecDeque::new(),
            max_ranges: 20,
            last_highlight_time: 0.0,
            debounce_ms: 50.0,
        }
    }
}

impl HighlightCache {
    pub fn should_debounce(&self, current_time: f64) -> bool {
        (current_time - self.last_highlight_time) < self.debounce_ms
    }

    pub fn mark_highlighted(&mut self, current_time: f64) {
        self.last_highlight_time = current_time;
    }

    /// Look up a cached highlight slice. Only `content_version` is checked —
    /// tree version changes are surfaced via the stale-entity rebuild path.
    pub fn get(
        &mut self,
        start_line: usize,
        end_line: usize,
        content_version: u64,
        _tree_version: u64,
    ) -> Option<Vec<Vec<LineSegment>>> {
        let mut found_idx: Option<(usize, usize, usize)> = None;
        for (idx, range) in self.ranges.iter().enumerate() {
            if range.content_version == content_version
                && range.start_line <= start_line
                && range.end_line >= end_line
            {
                let offset = start_line - range.start_line;
                let count = end_line - start_line;
                found_idx = Some((idx, offset, count));
                break;
            }
        }

        if let Some((idx, offset, count)) = found_idx {
            let result: Vec<Vec<LineSegment>> = self.ranges[idx]
                .lines
                .iter()
                .skip(offset)
                .take(count)
                .cloned()
                .collect();

            if idx > 0 {
                let range = self.ranges.remove(idx).unwrap();
                self.ranges.push_front(range);
            }

            Some(result)
        } else {
            None
        }
    }

    pub fn insert(
        &mut self,
        start_line: usize,
        end_line: usize,
        content_version: u64,
        _tree_version: u64,
        lines: Vec<Vec<LineSegment>>,
    ) {
        if self.ranges.len() >= self.max_ranges {
            self.ranges.pop_back();
        }

        self.ranges.push_front(CachedRange {
            start_line,
            end_line,
            content_version,
            lines,
        });
    }

    pub fn clear(&mut self) {
        self.ranges.clear();
    }
}

// ========== Async-parse adapter ==========
//
// Spawning + polling tasks lives in `bevy_tree_sitter::parse`. The editor:
//   1. detects when the buffer's `content_version` outruns the last parsed
//      version, snapshots provider state, and calls `spawn_parse`.
//   2. listens for `ParseCompleted` events and routes the new tree back into
//      `SyntaxResource`.

#[cfg(feature = "tree-sitter")]
pub(crate) fn spawn_async_parses(
    mut commands: Commands,
    mut editor_query: Query<
        (Entity, &mut SyntaxCacheState, &mut TextViewState),
        With<CodeEditor>,
    >,
    mut syntax: ResMut<SyntaxResource>,
    parse_tasks: Query<(), With<bevy_tree_sitter::ParseTask>>,
) {
    // Single in-flight parse at a time — multiple editors share `SyntaxResource`
    // (legacy single-editor assumption baked into `SyntaxResource`'s shape).
    if !parse_tasks.is_empty() {
        return;
    }

    for (entity, syntax_cache, tv) in editor_query.iter_mut() {
        if tv.content_version != syntax_cache.last_highlighted_version && syntax.is_available() {
            let rope = tv.rope.clone();
            let content_version = tv.content_version;

            let (parser, language, cached_tree) = syntax.clone_parse_state();

            bevy_tree_sitter::spawn_parse(
                &mut commands,
                entity,
                content_version,
                rope,
                parser,
                language,
                cached_tree,
            );
            // One parse per frame; consumers re-trigger next frame if needed.
            return;
        }
    }
}

#[cfg(feature = "tree-sitter")]
pub(crate) fn handle_parse_completed(
    mut events: MessageReader<ParseCompleted>,
    mut editor_query: Query<(&mut SyntaxCacheState, &mut TextViewState), With<CodeEditor>>,
    mut syntax: ResMut<SyntaxResource>,
    mut highlight_cache: ResMut<HighlightCache>,
) {
    for event in events.read() {
        let Ok((mut syntax_cache, mut tv)) = editor_query.get_mut(event.target) else {
            continue;
        };

        if let Some(tree) = event.tree.clone() {
            syntax.set_parsed_tree(tree, &tv.rope);
            syntax_cache.last_highlighted_version = event.content_version;

            highlight_cache.clear();

            // Bump content_version so the line glyph cache is fully invalidated
            // (otherwise cached uncolored glyphs from the pre-parse render persist).
            // Set last_highlighted_version to match so we don't re-trigger a parse.
            tv.content_version += 1;
            syntax_cache.last_highlighted_version = tv.content_version;
        }
    }
}

// ========== Edit Recording for Incremental Parsing ==========

#[cfg(feature = "tree-sitter")]
fn send_text_edit_events(
    mut editor_query: Query<(&mut SyntaxCacheState, &TextViewState), With<CodeEditor>>,
    mut writer: MessageWriter<crate::types::events::TextEditEvent>,
) {
    for (mut syntax_cache, tv) in editor_query.iter_mut() {
        if let Some((start_byte, old_end_byte, new_end_byte)) =
            syntax_cache.pending_tree_sitter_edit.take()
        {
            writer.write(crate::types::events::TextEditEvent::new(
                start_byte,
                old_end_byte,
                new_end_byte,
                tv.content_version,
            ));
        }
    }
}

#[cfg(feature = "tree-sitter")]
/// Apply edits synchronously to the cached tree (tree interpolation).
///
/// Runs after `send_text_edit_events`. By calling `tree.edit()` on the main
/// thread, the tree stays valid for highlighting queries while the async
/// re-parse runs in the background — eliminates the color flash on keystroke.
fn record_edits_for_incremental_parsing(
    editor_query: Query<(&SyntaxCacheState, &TextViewState), With<CodeEditor>>,
    mut syntax: ResMut<SyntaxResource>,
    mut events: MessageReader<crate::types::events::TextEditEvent>,
) {
    let collected_events: Vec<_> = events.read().cloned().collect();
    for (_syntax_cache, tv) in editor_query.iter() {
        for event in collected_events.iter() {
            // Sub-μs O(log n) rope lookups — fine to do on the main thread.
            let start_position = bevy_tree_sitter::parse::byte_to_point(&tv.rope, event.start_byte);
            let old_end_position =
                bevy_tree_sitter::parse::byte_to_point(&tv.rope, event.old_end_byte);
            let new_end_position =
                bevy_tree_sitter::parse::byte_to_point(&tv.rope, event.new_end_byte);

            let edit = bevy_tree_sitter::ts::InputEdit {
                start_byte: event.start_byte,
                old_end_byte: event.old_end_byte,
                new_end_byte: event.new_end_byte,
                start_position,
                old_end_position,
                new_end_position,
            };

            syntax.apply_sync_edit(edit, &tv.rope);
        }
    }
}

// ========== Plugin ==========

pub struct SyntaxPlugin;

impl Plugin for SyntaxPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SyntaxResource::new());

        app.insert_resource(HighlightCache::default());

        // TextEditEvent is editor-wide: LSP and other plugins listen for it.
        app.add_message::<crate::types::events::TextEditEvent>();

        #[cfg(feature = "tree-sitter")]
        {
            // Pull in the parse-task polling system + ParseCompleted event.
            if !app.is_plugin_added::<bevy_tree_sitter::TreeSitterPlugin>() {
                app.add_plugins(bevy_tree_sitter::TreeSitterPlugin);
            }

            // Edit pipeline: pending edits → events → sync `tree.edit()`.
            // Must run in ApplyStateSet so the tree is interpolated BEFORE
            // RenderingSet calls highlight_range().
            app.add_systems(
                Update,
                (send_text_edit_events, record_edits_for_incremental_parsing)
                    .chain()
                    .in_set(crate::plugin::ApplyStateSet),
            );

            // Async parse pipeline: kick off new parses, route completions back.
            app.add_systems(
                Update,
                (spawn_async_parses, handle_parse_completed)
                    .chain()
                    .in_set(crate::plugin::ApplyStateSet),
            );
        }
    }
}
