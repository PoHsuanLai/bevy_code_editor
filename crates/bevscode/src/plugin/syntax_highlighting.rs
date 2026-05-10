//! Editor-side syntax highlighting glue.
//!
//! The structural parsing + provider trait live in `bevy_tree_sitter`. This
//! module owns the editor-only pieces:
//!
//! - [`SyntaxInner`]: per-editor mutable state, an `Option<TreeSitterProvider>`
//!   plus a `tree_version` counter. Stored behind an `Arc<RwLock<_>>` so the
//!   parse pipeline (writers) and the editor's `produce_line_styles`
//!   producer (reader) can share access without each owning their own copy.
//! - `EditorParseSource` / `EditorBufferSnapshot`: bridge between the
//!   editor's `TextBuffer` (per-entity rope + version) and
//!   `bevy_tree_sitter`'s [`bevy_tree_sitter::ParseSource`] trait. The
//!   `parse_dirty` system in bevy_tree_sitter reads from this Component
//!   to drive async parses.
//! - `mirror_syntax_tree_to_provider`: editor system that filters on
//!   `Changed<bevy_tree_sitter::SyntaxTree>` and mirrors the freshly-parsed
//!   tree (plus its rope snapshot) into the per-entity provider so the
//!   styling layer's highlight queries find it. Also bumps
//!   `TextBuffer.content_version` to fully invalidate the glyph cache.
//! - [`init_editor_syntax`]: startup system that attaches the per-entity
//!   `SyntaxInner` Arc + the `EditorParseSource` Component and configures
//!   the provider's highlights query from a [`bevy_tree_sitter::Language`]
//!   Component (when one is present).

#[cfg(feature = "tree-sitter")]
use crate::text_view::TextBuffer;
use crate::types::CodeEditor;
use crate::types::LineSegment;
use bevy::prelude::*;
use std::sync::{Arc, RwLock};

#[cfg(feature = "tree-sitter")]
type InitEditorSyntaxQuery<'w, 's> = Query<
    'w,
    's,
    (Entity, Option<&'static bevy_tree_sitter::Language>),
    (With<CodeEditor>, Without<EditorSyntaxState>),
>;

#[cfg(feature = "tree-sitter")]
type ReactLanguageChangedQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static bevy_tree_sitter::Language,
        &'static EditorSyntaxState,
    ),
    (With<CodeEditor>, Changed<bevy_tree_sitter::Language>),
>;

#[cfg(feature = "tree-sitter")]
type SyncEditorParseSourceQuery<'w, 's> = Query<
    'w,
    's,
    (&'static TextBuffer, &'static EditorParseBufferRef),
    (With<CodeEditor>, Changed<TextBuffer>),
>;

#[cfg(feature = "tree-sitter")]
type MirrorSyntaxTreeQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static bevy_tree_sitter::SyntaxTree,
        &'static EditorSyntaxState,
        &'static TextBuffer,
    ),
    (With<CodeEditor>, Changed<bevy_tree_sitter::SyntaxTree>),
>;

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
    pub fn with_tree<R>(&self, f: impl FnOnce(&bevy_tree_sitter::ts::Tree) -> R) -> Option<R> {
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
            if kind.contains("string") || kind.contains("comment") || kind == "raw_string_literal" {
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
        theme: &crate::settings::SyntaxColors,
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
        _theme: &crate::settings::SyntaxColors,
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
/// capture names through the editor's `SyntaxColors`.
///
/// `start_byte` is the document byte offset of `text` — `HighlightRange`
/// byte ranges are document-absolute, so we subtract `start_byte` to index
/// into `text`.
#[cfg(feature = "tree-sitter")]
fn ranges_to_segments(
    text: &str,
    start_byte: usize,
    per_line: &[Vec<HighlightRange>],
    theme: &crate::settings::SyntaxColors,
    default_color: Color,
) -> Vec<Vec<LineSegment>> {
    let mut out: Vec<Vec<LineSegment>> =
        Vec::with_capacity(per_line.len().max(text.lines().count()));
    let mut byte_pos = 0usize;

    for (line, ranges) in text.lines().zip(per_line.iter()) {
        let line_start = byte_pos;
        let line_len = line.len();
        let line_end = line_start + line_len;

        // Walk the line in document bytes, slicing out segments where each
        // range starts/ends. Gaps between ranges become default-colored
        // segments. Ranges land sorted from the provider.
        //
        // Provider ranges are document-absolute (`start_byte + ...`); we
        // subtract `start_byte` to get the slice-relative byte offset before
        // clamping into [line_start, line_end]. Without this, every line
        // past the first one clamps every range to line_end and emits no
        // styled segments — the "only line 0 colored" bug.
        let mut segments: Vec<LineSegment> = Vec::with_capacity(ranges.len() + 1);
        let mut cursor = line_start;
        for range in ranges {
            let abs_to_slice = |b: usize| b.saturating_sub(start_byte);
            let range_start = abs_to_slice(range.byte_range.start)
                .max(line_start)
                .min(line_end);
            let range_end = abs_to_slice(range.byte_range.end)
                .max(line_start)
                .min(line_end);

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

    // Pad with empty rows if the provider returned fewer than expected.
    while out.len() < per_line.len() {
        out.push(Vec::new());
    }
    out
}

/// Snapshot of an editor's buffer state — the slice the parser actually
/// reads. Held behind `Arc<RwLock<_>>` so:
///
/// - The `sync_editor_parse_source` system updates it each frame from
///   the entity's `TextBuffer`.
/// - The `bevy_tree_sitter::parse_dirty` system reads `content_version` +
///   `snapshot` from it (through the `EditorParseSource`'s `ParseSource`
///   impl) to decide whether to kick off a new parse, and what rope to
///   feed to the worker.
///
/// The version stored here lags `TextBuffer.content_version` by at
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
pub fn init_editor_syntax(mut commands: Commands, editors: InitEditorSyntaxQuery) {
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
pub(crate) fn react_language_changed(editors: ReactLanguageChangedQuery) {
    for (language, syntax_state) in editors.iter() {
        let Some(new_provider) = language.create_tree_sitter_provider() else {
            continue;
        };
        syntax_state.inner.write().unwrap().provider = Some(new_provider);
    }
}

#[cfg(feature = "tree-sitter")]
/// Mirror `TextBuffer.rope` + `content_version` into the per-entity
/// `EditorBufferSnapshot` so the next `parse_dirty` tick sees the latest
/// content. Runs in `ApplyStateSet` after edits land on `TextBuffer`.
pub(crate) fn sync_editor_parse_source(editors: SyncEditorParseSourceQuery) {
    for (buffer, buf_ref) in editors.iter() {
        let mut buf = buf_ref.0.write().unwrap();
        // Hot path: only write if something changed. RwLock writes are
        // cheap but the rope clone in particular is one Arc bump we'd
        // rather skip when nothing's changed.
        if buf.content_version == buffer.content_version {
            continue;
        }
        buf.rope = buffer.rope.clone();
        buf.content_version = buffer.content_version;
    }
}

#[cfg(feature = "tree-sitter")]
/// React to a freshly-completed parse by mirroring the new tree (and its
/// rope) into the per-entity provider so highlight queries find it.
/// Loop prevention lives on `EditorSyntaxState::applied_content_version`.
pub(crate) fn mirror_syntax_tree_to_provider(mut editor_query: MirrorSyntaxTreeQuery) {
    for (syntax_tree, syntax_state, buffer) in editor_query.iter_mut() {
        let Some(tree) = syntax_tree.tree.as_ref() else {
            continue;
        };

        let mut guard = syntax_state.inner.write().unwrap();
        let inner = &mut *guard;
        // If a sync re-parse already produced a fresher tree, skip — the
        // async result is stale relative to the live state. (Initial mirror
        // always proceeds: provider.cached_tree is None.)
        let provider_has_tree = inner
            .provider
            .as_ref()
            .map(|p| p.cached_tree.is_some())
            .unwrap_or(false);
        if provider_has_tree && syntax_tree.content_version <= inner.applied_content_version {
            continue;
        }
        if let Some(provider) = &mut inner.provider {
            provider.cached_tree = Some(tree.clone());
            provider.cached_rope = Some(buffer.rope.clone());
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
    }
}

#[cfg(feature = "tree-sitter")]
/// Apply edits synchronously to the cached tree (tree interpolation).
///
/// Reads [`crate::types::events::TextEdited`] (emitted by the
/// `on_edit_invalidate_caches` observer). Routes through
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
    mut events: MessageReader<crate::types::events::TextEdited>,
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

            // Skip "tree interpolation" for huge edits. `tree.edit()` is
            // O(log n) per leaf, but a select-all-delete touches every leaf
            // in a 7 MB tree (~60 ms). The async `parse_dirty` will replace
            // both trees with a fresh parse a frame later anyway, so a frame
            // of stale highlights costs nothing and skipping the dual
            // `tree.edit()` calls saves the freeze.
            let removed = edit.old_end_byte.saturating_sub(edit.start_byte);
            let inserted = edit.new_end_byte.saturating_sub(edit.start_byte);
            let huge_edit = removed > bevy_tree_sitter::SYNC_REPARSE_BYTE_LIMIT
                || inserted > bevy_tree_sitter::SYNC_REPARSE_BYTE_LIMIT;

            if !huge_edit {
                let st = syntax_tree.bypass_change_detection();
                if let Some(tree) = st.tree.as_mut() {
                    tree.edit(&edit);
                }
                // Union the edit's row range into dirty_rows so the async
                // parse completion can forward it to produce_line_styles for
                // incremental rehighlight instead of a full-window rebuild.
                let start_row = edit.start_position.row as u32;
                let end_row = edit.new_end_position.row as u32;
                st.dirty_rows = Some(match st.dirty_rows {
                    Some((lo, hi)) => (lo.min(start_row), hi.max(end_row)),
                    None => (start_row, end_row),
                });
                parse_source.0.apply_edit(edit);
            } else {
                // For huge edits, drop the cached trees entirely so any
                // in-flight highlight query sees "no tree" and falls back
                // to plain text instead of querying byte-shifted-but-stale
                // structure. The async reparse will repopulate.
                let st = syntax_tree.bypass_change_detection();
                st.tree = None;
                // Full rebuild needed — clear dirty_rows.
                st.dirty_rows = None;
                if let Some(provider) = &mut syntax_state.inner.write().unwrap().provider {
                    provider.invalidate_tree();
                }
            }
            syntax_state.inner.write().unwrap().applied_content_version = event.content_version;
        }
    }
}

pub struct SyntaxPlugin;

impl Plugin for SyntaxPlugin {
    fn build(&self, app: &mut App) {
        // TextEdited is editor-wide: LSP and other plugins listen for it.
        app.add_message::<crate::types::events::TextEdited>();

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
        app.add_systems(
            Update,
            init_editor_syntax.in_set(crate::plugin::ApplyStateSet),
        );

        #[cfg(feature = "tree-sitter")]
        {
            // Pull in the parse-driving system. Idempotent if the host
            // already added the plugin.
            if !app.is_plugin_added::<bevy_tree_sitter::TreeSitterPlugin>() {
                app.add_plugins(bevy_tree_sitter::TreeSitterPlugin);
            }

            // Edit pipeline ordering:
            //   1. react_language_changed: install provider
            //   2. sync_editor_parse_source: mirror buffer.rope into the parse-source snapshot
            //   3. record_edits_for_incremental_parsing: tree.edit() + sync re-parse,
            //      reading TextEdited emitted by the on_edit_invalidate_caches observer
            //   4. parse_dirty (ParseSet): async re-parse with the synced rope
            // Steps 2 and 3 must be ordered: 3 reads the rope mirror via
            // ParseSourceComp::apply_edit, so 2 must mirror the post-edit
            // rope first.
            app.add_systems(
                Update,
                (
                    react_language_changed,
                    sync_editor_parse_source,
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
