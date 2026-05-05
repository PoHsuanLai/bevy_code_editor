//! Editor-side syntax highlighting glue.
//!
//! The structural parsing + provider trait live in `bevy_tree_sitter`. This
//! module owns the editor-only pieces:
//!
//! - [`SyntaxInner`]: per-editor mutable state, an `Option<TreeSitterProvider>`
//!   plus a `tree_version` counter. Stored behind an `Arc<RwLock<_>>` so the
//!   parse pipeline (writers) and the editor's `produce_line_styles`
//!   producer (reader) can share access without each owning their own copy.
//! - [`EditorParseSource`] / [`EditorBufferSnapshot`]: bridge between the
//!   editor's `TextViewState` (per-entity rope + version) and
//!   `bevy_tree_sitter`'s [`bevy_tree_sitter::ParseSource`] trait. The
//!   `parse_dirty` system in bevy_tree_sitter reads from this Component
//!   to drive async parses.
//! - [`mirror_syntax_tree_to_provider`]: editor system that filters on
//!   `Changed<bevy_tree_sitter::SyntaxTree>` and mirrors the freshly-parsed
//!   tree (plus its rope snapshot) into the per-entity provider so the
//!   styling layer's highlight queries find it. Also bumps
//!   `TextViewState.content_version` to fully invalidate the glyph cache.
//! - [`init_editor_syntax`]: startup system that attaches the per-entity
//!   `SyntaxInner` Arc + the `EditorParseSource` Component and configures
//!   the provider's highlights query from a [`bevy_tree_sitter::Language`]
//!   Component (when one is present).

use crate::types::CodeEditor;
use crate::types::LineSegment;
#[cfg(feature = "tree-sitter")]
use crate::text_view::TextViewState;
#[cfg(feature = "tree-sitter")]
use crate::types::SyntaxCacheState;
use bevy::prelude::*;
use std::sync::{Arc, RwLock};

#[cfg(feature = "tree-sitter")]
use bevy_tree_sitter::{HighlightRange, SyntaxProvider, TreeSitterProvider};

/// Mutable state inside [`EditorSyntaxState`]. Held behind an `Arc<RwLock<_>>`
/// so the editor's parse pipeline (writers) and the styling producer
/// (reader) can share access without each owning their own copy.
pub struct SyntaxInner {
    #[cfg(feature = "tree-sitter")]
    pub(crate) provider: Option<TreeSitterProvider>,
    #[cfg(feature = "tree-sitter")]
    pub(crate) tree_version: u64,
    /// Content version of the rope last fed to the provider's cached tree,
    /// either via sync re-parse (`record_edits_for_incremental_parsing`)
    /// or via the async parse mirror. Used by the mirror to skip
    /// overwriting a fresher cached tree with an older async result.
    #[cfg(feature = "tree-sitter")]
    pub(crate) applied_content_version: u64,
}

impl SyntaxInner {
    fn new() -> Self {
        Self {
            #[cfg(feature = "tree-sitter")]
            provider: None,
            #[cfg(feature = "tree-sitter")]
            tree_version: 0,
            #[cfg(feature = "tree-sitter")]
            applied_content_version: 0,
        }
    }
}

/// Per-entity Component holding the editor's syntax-provider state.
///
/// Public so host setup code (e.g. the LSP example) can install a provider
/// directly. Day-to-day usage only needs to attach a
/// [`bevy_tree_sitter::Language`] Component — [`init_editor_syntax`] picks
/// it up at startup and configures the provider's highlights query.
// not reflectable: holds `TreeSitterProvider` which wraps `tree_sitter::*`
// types that don't implement `Reflect`.
#[derive(Component, Clone)]
pub struct EditorSyntaxState {
    pub(crate) inner: Arc<RwLock<SyntaxInner>>,
}

impl EditorSyntaxState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(SyntaxInner::new())),
        }
    }

    /// Hand out a clone of the inner `Arc<RwLock<_>>`. Cheap (refcount bump).
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

    /// True when `byte_offset` is somewhere a completion request makes
    /// sense — i.e. *not* inside a string literal or comment. Returns
    /// `true` (allow) when we have no tree or the language doesn't define
    /// string/comment node kinds; the caller falls back to its prefix /
    /// trigger heuristics. Mirrors Zed's "skip in string/comment" gate.
    #[cfg(feature = "tree-sitter")]
    pub fn is_completion_context(&self, byte_offset: usize) -> bool {
        let guard = self.inner.read().unwrap();
        let Some(provider) = guard.provider.as_ref() else {
            return true;
        };
        let Some(tree) = provider.tree() else {
            return true;
        };
        let root = tree.root_node();
        if byte_offset > root.end_byte() {
            return true;
        }
        let Some(node) = root.descendant_for_byte_range(byte_offset, byte_offset) else {
            return true;
        };
        let mut cur = Some(node);
        while let Some(n) = cur {
            let kind = n.kind();
            // Conservative match: tree-sitter grammars name these consistently
            // across the languages we ship (rust, javascript, python, …).
            if kind.contains("string") || kind.contains("comment") || kind == "raw_string_literal"
            {
                return false;
            }
            cur = n.parent();
        }
        true
    }

    #[cfg(not(feature = "tree-sitter"))]
    pub fn is_completion_context(&self, _byte_offset: usize) -> bool {
        true
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
}

impl Default for EditorSyntaxState {
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


// ========== ParseSource bridge ==========

/// Snapshot of an editor's buffer state — the slice the parser actually
/// reads. Held behind `Arc<RwLock<_>>` so:
///
/// - The `sync_editor_parse_source` system updates it each frame from
///   the entity's `TextViewState`.
/// - The `bevy_tree_sitter::parse_dirty` system reads `content_version` +
///   `snapshot` from it (through the `EditorParseSource`'s `ParseSource`
///   impl) to decide whether to kick off a new parse, and what rope to
///   feed to the worker.
///
/// The version stored here lags `TextViewState.content_version` by at
/// most one frame — same staleness profile as the old global-resource
/// pipeline.
#[cfg(feature = "tree-sitter")]
#[derive(Default)]
pub(crate) struct EditorBufferSnapshot {
    pub(crate) rope: ropey::Rope,
    pub(crate) content_version: u64,
}

/// `ParseSource` impl wired into `bevy_tree_sitter`'s parse pipeline. Holds
/// the per-entity buffer snapshot + the per-entity `SyntaxInner` so
/// `apply_edit` can interpolate the cached tree without a separate Bevy
/// system call.
#[cfg(feature = "tree-sitter")]
pub(crate) struct EditorParseSource {
    pub(crate) buf: Arc<RwLock<EditorBufferSnapshot>>,
    pub(crate) syntax: Arc<RwLock<SyntaxInner>>,
}

#[cfg(feature = "tree-sitter")]
impl bevy_tree_sitter::ParseSource for EditorParseSource {
    fn content_version(&self) -> u64 {
        self.buf.read().unwrap().content_version
    }

    fn snapshot(&self) -> ropey::Rope {
        self.buf.read().unwrap().rope.clone()
    }

    fn apply_edit(&self, edit: bevy_tree_sitter::ts::InputEdit) {
        // Tree interpolation: shift the cached tree's byte offsets so the
        // highlight queries see consistent ranges while the next async
        // re-parse runs. Mirrors the old `TreeSitterProvider::apply_sync_edit`
        // — we keep the rope snapshot through the buffer mirror.
        let rope = self.buf.read().unwrap().rope.clone();
        if let Some(provider) = &mut self.syntax.write().unwrap().provider {
            provider.apply_sync_edit(edit, &rope);
        }
    }
}

// ========== Pipeline systems ==========

/// On startup, attach `EditorSyntaxState` + (when tree-sitter feature is
/// on) `bevy_tree_sitter::SyntaxTree` + `bevy_tree_sitter::ParseSourceComp`
/// to every CodeEditor entity.
///
/// Each editor gets its own `Arc<RwLock<SyntaxInner>>` — the shared state
/// the styling layer reads and the parse pipeline mirrors trees into.
/// The `Language` Component, if already present, drives the provider's
/// highlights-query setup; otherwise the editor falls back to plain text
/// until a host setup system installs one.
#[cfg(feature = "tree-sitter")]
pub fn init_editor_syntax(
    mut commands: Commands,
    editors: Query<
        (Entity, Option<&bevy_tree_sitter::Language>),
        (With<CodeEditor>, Without<EditorSyntaxState>),
    >,
) {
    for (entity, language) in editors.iter() {
        let syntax_state = EditorSyntaxState::new();

        // If a Language Component is already present, configure the
        // provider's highlights query so styling can run on the first
        // parse completion. Hosts that don't use the Component-driven
        // path can call `EditorSyntaxState::set_provider` themselves.
        if let Some(lang) = language {
            if let Some(provider) = lang.create_tree_sitter_provider() {
                syntax_state.inner.write().unwrap().provider = Some(provider);
            }
        }

        let buf = Arc::new(RwLock::new(EditorBufferSnapshot::default()));
        let parse_source = EditorParseSource {
            buf: buf.clone(),
            syntax: syntax_state.inner.clone(),
        };

        commands.entity(entity).insert((
            syntax_state,
            EditorParseBufferRef(buf),
            bevy_tree_sitter::ParseSourceComp::new(parse_source),
            bevy_tree_sitter::SyntaxTree::default(),
        ));
    }
}

/// No-tree-sitter fallback: just install `EditorSyntaxState` so the
/// styling plumbing has something to share. Provider stays `None`; styling
/// returns empty runs.
#[cfg(not(feature = "tree-sitter"))]
pub fn init_editor_syntax(
    mut commands: Commands,
    editors: Query<Entity, (With<CodeEditor>, Without<EditorSyntaxState>)>,
) {
    for entity in editors.iter() {
        commands.entity(entity).insert(EditorSyntaxState::new());
    }
}

/// Per-entity handle to the `EditorBufferSnapshot` the `ParseSource` reads
/// from. The sync system writes through this without needing to downcast
/// the `dyn ParseSource`.
#[cfg(feature = "tree-sitter")]
#[derive(Component)]
pub(crate) struct EditorParseBufferRef(pub(crate) Arc<RwLock<EditorBufferSnapshot>>);

/// React to a [`bevy_tree_sitter::Language`] change on an editor entity by
/// (re-)configuring the provider's highlights query. Lets host setup code
/// install a `Language` post-`init_editor_syntax` and have it picked up.
#[cfg(feature = "tree-sitter")]
pub(crate) fn react_language_changed(
    editors: Query<
        (&bevy_tree_sitter::Language, &EditorSyntaxState),
        (With<CodeEditor>, Changed<bevy_tree_sitter::Language>),
    >,
) {
    for (language, syntax_state) in editors.iter() {
        let Some(new_provider) = language.create_tree_sitter_provider() else {
            continue;
        };
        syntax_state.inner.write().unwrap().provider = Some(new_provider);
    }
}

#[cfg(feature = "tree-sitter")]
/// Mirror `TextViewState.rope` + `content_version` into the per-entity
/// `EditorBufferSnapshot` so the next `parse_dirty` tick sees the latest
/// content. Runs in `ApplyStateSet` after edits land on `TextViewState`.
pub(crate) fn sync_editor_parse_source(
    editors: Query<(&TextViewState, &EditorParseBufferRef), With<CodeEditor>>,
) {
    for (tv, buf_ref) in editors.iter() {
        let mut buf = buf_ref.0.write().unwrap();
        // Hot path: only write if something changed. RwLock writes are
        // cheap but the rope clone in particular is one Arc bump we'd
        // rather skip when nothing's changed.
        if buf.content_version == tv.content_version {
            continue;
        }
        buf.rope = tv.rope.clone();
        buf.content_version = tv.content_version;
    }
}

#[cfg(feature = "tree-sitter")]
/// React to a freshly-completed parse — written by `bevy_tree_sitter`'s
/// `parse_dirty` into the entity's [`bevy_tree_sitter::SyntaxTree`] —
/// by mirroring the new tree (and its rope) into the per-entity provider
/// so highlight queries find it.
///
/// Mirrors the old `handle_parse_completed`: bumps `content_version` so the
/// line-glyph cache fully invalidates (otherwise pre-parse plain-color glyph
/// runs persist on screen). Sets `last_highlighted_version` to match the
/// bumped version so we don't trigger an immediate re-parse loop.
pub(crate) fn mirror_syntax_tree_to_provider(
    mut editor_query: Query<
        (
            &bevy_tree_sitter::SyntaxTree,
            &EditorSyntaxState,
            &mut SyntaxCacheState,
            &TextViewState,
        ),
        (With<CodeEditor>, Changed<bevy_tree_sitter::SyntaxTree>),
    >,
) {
    for (syntax_tree, syntax_state, mut syntax_cache, tv) in editor_query.iter_mut() {
        let Some(tree) = syntax_tree.tree.as_ref() else {
            continue;
        };

        let mut guard = syntax_state.inner.write().unwrap();
        let inner = &mut *guard;
        // If a sync re-parse already produced a fresher tree, skip — the
        // async result is stale relative to the live state. (Initial mirror
        // always proceeds: provider.cached_tree is None.)
        let provider_has_tree = inner.provider.as_ref()
            .map(|p| p.cached_tree.is_some())
            .unwrap_or(false);
        if provider_has_tree && syntax_tree.content_version <= inner.applied_content_version {
            continue;
        }
        if let Some(provider) = &mut inner.provider {
            provider.cached_tree = Some(tree.clone());
            provider.cached_rope = Some(tv.rope.clone());
            if provider.cached_parser.is_none() {
                if let Some(ref language) = provider.cached_language {
                    let mut parser = bevy_tree_sitter::ts::Parser::new();
                    if parser.set_language(language).is_ok() {
                        provider.cached_parser = Some(parser);
                    }
                }
            }
            inner.tree_version = inner.tree_version.wrapping_add(1);
            inner.applied_content_version = syntax_tree.content_version;
        }
        drop(guard);

        syntax_cache.last_highlighted_version = syntax_tree.content_version;
        // Don't touch `tv.content_version`. The downstream chain is:
        // `Changed<SyntaxTree>` (this system's filter writes a fresh tick)
        // → `produce_line_styles` re-styles → `LineStyles` Arc swaps →
        // engine refingerprints and rebuilds the layout. Bumping
        // `content_version` here used to feed back through
        // `sync_editor_parse_source` into a runaway re-parse loop.
    }
}

// ========== Edit Recording for Incremental Parsing ==========

#[cfg(feature = "tree-sitter")]
fn send_text_edit_events(
    mut editor_query: Query<(&mut SyntaxCacheState, &TextViewState), With<CodeEditor>>,
    mut writer: MessageWriter<crate::types::events::TextEditEvent>,
) {
    for (mut syntax_cache, tv) in editor_query.iter_mut() {
        if let Some(delta) = syntax_cache.pending_tree_sitter_edit.take() {
            let pre = syntax_cache.pending_pre_edit_rope.take();
            writer.write(
                crate::types::events::TextEditEvent::new(delta, tv.content_version)
                    .with_pre_edit_rope(pre),
            );
        }
    }
}

#[cfg(feature = "tree-sitter")]
/// Apply edits synchronously to the cached tree (tree interpolation).
///
/// Runs after `send_text_edit_events`. Routes through
/// [`bevy_tree_sitter::ParseSource::apply_edit`] on the editor's
/// `ParseSourceComp` — the editor's impl forwards to the per-entity
/// provider's `apply_sync_edit`. Tree stays valid for highlighting queries
/// while the async re-parse runs in the background — eliminates the color
/// flash on keystroke.
fn record_edits_for_incremental_parsing(
    mut editor_query: Query<
        (
            &bevy_tree_sitter::ParseSourceComp,
            &mut bevy_tree_sitter::SyntaxTree,
            &EditorSyntaxState,
        ),
        With<CodeEditor>,
    >,
    mut events: MessageReader<crate::types::events::TextEditEvent>,
) {
    let collected_events: Vec<_> = events.read().cloned().collect();
    for (parse_source, mut syntax_tree, syntax_state) in editor_query.iter_mut() {
        for event in collected_events.iter() {
            let d = &event.delta;
            let edit = bevy_tree_sitter::ts::InputEdit {
                start_byte: d.start_byte,
                old_end_byte: d.old_end_byte,
                new_end_byte: d.new_end_byte,
                start_position: bevy_tree_sitter::ts::Point::new(
                    d.start_position.row as usize,
                    d.start_position.column_byte as usize,
                ),
                old_end_position: bevy_tree_sitter::ts::Point::new(
                    d.old_end_position.row as usize,
                    d.old_end_position.column_byte as usize,
                ),
                new_end_position: bevy_tree_sitter::ts::Point::new(
                    d.new_end_position.row as usize,
                    d.new_end_position.column_byte as usize,
                ),
            };
            let st = syntax_tree.bypass_change_detection();
            if let Some(tree) = st.tree.as_mut() {
                tree.edit(&edit);
            }
            parse_source.0.apply_edit(edit);
            syntax_state.inner.write().unwrap().applied_content_version = event.content_version;
        }
    }
}

// ========== Plugin ==========

pub struct SyntaxPlugin;

impl Plugin for SyntaxPlugin {
    fn build(&self, app: &mut App) {
        // TextEditEvent is editor-wide: LSP and other plugins listen for it.
        app.add_message::<crate::types::events::TextEditEvent>();

        // Always attach `EditorSyntaxState` to each editor entity — the
        // styling plumbing needs an Arc to share regardless of whether
        // tree-sitter is wired up. Runs in both Startup (for editors
        // spawned during plugin setup) and Update (for editors spawned at
        // runtime). The query filter `Without<EditorSyntaxState>` makes it
        // idempotent — once attached, it's a no-op.
        app.add_systems(
            Startup,
            init_editor_syntax.after(crate::plugin::spawn_editor_entity),
        );
        app.add_systems(Update, init_editor_syntax.in_set(crate::plugin::ApplyStateSet));

        #[cfg(feature = "tree-sitter")]
        {
            // Pull in the parse-driving system. Idempotent if the host
            // already added the plugin.
            if !app.is_plugin_added::<bevy_tree_sitter::TreeSitterPlugin>() {
                app.add_plugins(bevy_tree_sitter::TreeSitterPlugin);
            }

            // Edit pipeline ordering:
            //   1. react_language_changed: install provider
            //   2. sync_editor_parse_source: mirror tv.rope into buffer snapshot
            //   3. send_text_edit_events: drain SyntaxCacheState.pending_tree_sitter_edit
            //   4. record_edits_for_incremental_parsing: tree.edit() + sync re-parse
            //   5. parse_dirty (ParseSet): kicks off async re-parse with the synced rope
            // Steps 2 and 4 must be ordered: 4 reads the rope mirror via
            // ParseSourceComp::apply_edit, so 2 must mirror the post-edit
            // rope first.
            app.add_systems(
                Update,
                (
                    react_language_changed,
                    sync_editor_parse_source,
                    send_text_edit_events,
                    record_edits_for_incremental_parsing,
                )
                    .chain()
                    .in_set(crate::plugin::ApplyStateSet)
                    .before(bevy_tree_sitter::ParseSet),
            );

            app.add_systems(
                Update,
                mirror_syntax_tree_to_provider
                    .in_set(crate::plugin::ApplyStateSet)
                    .after(bevy_tree_sitter::ParseSet),
            );
        }
    }
}
