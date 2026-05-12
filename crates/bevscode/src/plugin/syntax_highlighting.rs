//! Editor-side syntax highlighting glue.
//!
//! The structural parsing lives in `bevy_tree_sitter`. This module owns the
//! editor-only pieces:
//!
//! - [`EditorSyntaxState`]: a simple Component wrapping `Option<TreeSitterProvider>`
//!   (the compiled query + cursor). No Arc, no RwLock — it's an ordinary ECS
//!   component queried directly by the systems that need it.
//! - `EditorParseSource` / `EditorBufferSnapshot`: bridge between the editor's
//!   `TextBuffer` and `bevy_tree_sitter`'s `ParseSource` trait. Drives async parses.
//! - [`init_editor_syntax`]: startup system that attaches `EditorSyntaxState` +
//!   `ParseSourceComp` + `SyntaxTree` and configures the provider's highlights
//!   query from a `TreeSitterGrammar` component when one is present.
//!
//! `SyntaxTree` (written by `parse_dirty`) is the single source of truth for the
//! parsed tree. Highlighting queries read from it directly — no mirror system.

#[cfg(feature = "tree-sitter")]
use crate::text_view::TextBuffer;
use crate::types::CodeEditor;
use crate::types::LineSegment;
use bevy::prelude::*;

#[cfg(feature = "tree-sitter")]
use std::sync::{Arc, RwLock};

#[cfg(feature = "tree-sitter")]
type InitEditorSyntaxQuery<'w, 's> = Query<
    'w,
    's,
    (Entity, Option<&'static bevy_tree_sitter::TreeSitterGrammar>),
    (With<CodeEditor>, Without<EditorSyntaxState>),
>;

#[cfg(feature = "tree-sitter")]
type ReactLanguageChangedQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static bevy_tree_sitter::TreeSitterGrammar,
        &'static mut EditorSyntaxState,
    ),
    (With<CodeEditor>, Changed<bevy_tree_sitter::TreeSitterGrammar>),
>;

#[cfg(feature = "tree-sitter")]
type SyncEditorParseSourceQuery<'w, 's> = Query<
    'w,
    's,
    (&'static TextBuffer, &'static EditorParseBufferRef),
    With<CodeEditor>,
>;

/// Per-entity Component holding the compiled highlight query provider.
///
/// An ordinary ECS component — no Arc, no RwLock. Systems that need it query
/// `&mut EditorSyntaxState` directly. `SyntaxTree` (written by `parse_dirty`)
/// is the single source of truth for the parsed tree.
///
/// Not reflectable: holds `TreeSitterProvider` which wraps `tree_sitter::*`
/// types that don't implement `Reflect`.
#[derive(Component, Default)]
pub struct EditorSyntaxState {
    #[cfg(feature = "tree-sitter")]
    pub(crate) provider: Option<bevy_tree_sitter::TreeSitterProvider>,
}

impl EditorSyntaxState {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(feature = "tree-sitter")]
    pub fn set_provider(&mut self, provider: bevy_tree_sitter::TreeSitterProvider) {
        self.provider = Some(provider);
    }

    pub fn is_available(&self) -> bool {
        #[cfg(feature = "tree-sitter")]
        {
            self.provider.as_ref().map(|p| p.is_available()).unwrap_or(false)
        }
        #[cfg(not(feature = "tree-sitter"))]
        {
            false
        }
    }

    /// True when `byte_offset` is somewhere a completion request makes
    /// sense — i.e. *not* inside a string literal or comment. Callers pass
    /// the tree from `SyntaxTree` directly; returns `true` when absent.
    #[cfg(feature = "tree-sitter")]
    pub fn is_completion_context(
        tree: &bevy_tree_sitter::ts::Tree,
        byte_offset: usize,
    ) -> bool {
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
            if kind.contains("string") || kind.contains("comment") || kind == "raw_string_literal" {
                return false;
            }
            cur = n.parent();
        }
        true
    }

    #[cfg(not(feature = "tree-sitter"))]
    pub fn is_completion_context(_byte_offset: usize) -> bool {
        true
    }

    /// Highlight `text` and return styled per-line segments.
    ///
    /// Reads the tree directly from `syntax_tree` — no internal tree cache.
    /// Returns plain-text segments when the provider or tree is absent.
    #[cfg(feature = "tree-sitter")]
    pub fn highlight_range(
        &mut self,
        text: &str,
        start_byte: usize,
        syntax_tree: &bevy_tree_sitter::SyntaxTree,
        rope: &ropey::Rope,
        theme: &crate::settings::SyntaxColors,
        default_color: Color,
    ) -> Vec<Vec<LineSegment>> {
        let Some(provider) = &mut self.provider else {
            return plain_text_segments(text, default_color);
        };
        let Some(tree) = syntax_tree.tree.as_ref() else {
            return plain_text_segments(text, default_color);
        };
        let end_byte = start_byte + text.len();
        match provider.highlight_range(tree, rope, start_byte..end_byte) {
            Some(highlights) => ranges_to_segments(text, start_byte, &highlights, theme, default_color),
            None => plain_text_segments(text, default_color),
        }
    }

    #[cfg(not(feature = "tree-sitter"))]
    pub fn highlight_range(
        &mut self,
        text: &str,
        _start_byte: usize,
        _theme: &crate::settings::SyntaxColors,
        default_color: Color,
    ) -> Vec<Vec<LineSegment>> {
        plain_text_segments(text, default_color)
    }
}

/// Build LineSegments for `text` with no highlights — fallback for when no
/// provider or tree is available.
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

/// Translate a flat sorted `HighlightRange` slice into per-line `LineSegment`s,
/// mapping capture names through the editor's `SyntaxColors`.
///
/// `highlights` is document-absolute and sorted by `byte_range.start`.
/// `start_byte` is the document byte offset of the first byte of `text`.
/// Two-pointer walk: O(L + H) where L = bytes in text, H = highlight count.
#[cfg(feature = "tree-sitter")]
fn ranges_to_segments(
    text: &str,
    start_byte: usize,
    highlights: &[bevy_tree_sitter::HighlightRange],
    theme: &crate::settings::SyntaxColors,
    default_color: Color,
) -> Vec<Vec<LineSegment>> {
    let mut out: Vec<Vec<LineSegment>> = Vec::with_capacity(text.lines().count());
    let mut byte_pos = 0usize;
    let mut hi_idx = 0usize;

    for line in text.lines() {
        let line_start = byte_pos;
        let line_len = line.len();
        let line_end = line_start + line_len;
        let abs_line_start = start_byte + line_start;
        let abs_line_end = start_byte + line_end;

        while hi_idx < highlights.len()
            && highlights[hi_idx].byte_range.end <= abs_line_start
        {
            hi_idx += 1;
        }

        let mut segments: Vec<LineSegment> = Vec::new();
        let mut cursor = abs_line_start;
        let mut local_hi = hi_idx;

        while cursor < abs_line_end {
            while local_hi < highlights.len()
                && highlights[local_hi].byte_range.end <= cursor
            {
                local_hi += 1;
            }

            if local_hi < highlights.len() {
                let hl = &highlights[local_hi];
                let hl_start = hl.byte_range.start.max(abs_line_start);
                let hl_end = hl.byte_range.end.min(abs_line_end);

                if hl_start > cursor {
                    let lo = cursor - abs_line_start;
                    let hi = hl_start - abs_line_start;
                    let slice = &line[lo..hi];
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
                    cursor = hl_start;
                } else {
                    let lo = cursor - abs_line_start;
                    let hi = hl_end - abs_line_start;
                    let slice = &line[lo..hi];
                    if !slice.is_empty() {
                        let color = crate::syntax::map_highlight_color(
                            Some(&hl.capture_name),
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
                    cursor = hl_end;
                    local_hi += 1;
                }
            } else {
                let lo = cursor - abs_line_start;
                let slice = &line[lo..];
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
                cursor = abs_line_end;
            }
        }

        if segments.iter().all(|s| s.text.trim().is_empty()) {
            out.push(Vec::new());
        } else {
            out.push(segments);
        }

        byte_pos = line_end + 1;
    }

    out
}

/// Snapshot of an editor's buffer state for the async parse pipeline.
#[cfg(feature = "tree-sitter")]
#[derive(Default)]
pub(crate) struct EditorBufferSnapshot {
    pub(crate) rope: ropey::Rope,
    pub(crate) content_version: u64,
}

/// `ParseSource` impl wired into `bevy_tree_sitter`'s parse pipeline.
/// Only carries the buffer snapshot — no tree state.
#[cfg(feature = "tree-sitter")]
pub(crate) struct EditorParseSource {
    pub(crate) buf: Arc<RwLock<EditorBufferSnapshot>>,
}

#[cfg(feature = "tree-sitter")]
impl bevy_tree_sitter::ParseSource for EditorParseSource {
    fn content_version(&self) -> u64 {
        self.buf.read().unwrap().content_version
    }

    fn snapshot(&self) -> ropey::Rope {
        self.buf.read().unwrap().rope.clone()
    }
}

/// On startup, attach `EditorSyntaxState` + `ParseSourceComp` + `SyntaxTree`
/// to every `CodeEditor` entity. `TreeSitterGrammar`, if present, drives the
/// initial provider setup.
#[cfg(feature = "tree-sitter")]
pub fn init_editor_syntax(mut commands: Commands, editors: InitEditorSyntaxQuery) {
    for (entity, grammar) in editors.iter() {
        let mut syntax_state = EditorSyntaxState::new();

        if let Some(g) = grammar {
            if let Some(provider) = g.create_provider() {
                syntax_state.provider = Some(provider);
            }
        }

        let buf = Arc::new(RwLock::new(EditorBufferSnapshot::default()));
        let parse_source = EditorParseSource { buf: buf.clone() };

        commands.entity(entity).insert((
            syntax_state,
            EditorParseBufferRef(buf),
            bevy_tree_sitter::ParseSourceComp::new(parse_source),
            bevy_tree_sitter::SyntaxTree::default(),
        ));
    }
}

#[cfg(not(feature = "tree-sitter"))]
pub fn init_editor_syntax(
    mut commands: Commands,
    editors: Query<Entity, (With<CodeEditor>, Without<EditorSyntaxState>)>,
) {
    for entity in editors.iter() {
        commands.entity(entity).insert(EditorSyntaxState::new());
    }
}

/// Per-entity handle to the `EditorBufferSnapshot` the `ParseSource` reads from.
#[cfg(feature = "tree-sitter")]
#[derive(Component)]
pub(crate) struct EditorParseBufferRef(pub(crate) Arc<RwLock<EditorBufferSnapshot>>);

/// React to a `TreeSitterGrammar` change by (re-)configuring the provider.
#[cfg(feature = "tree-sitter")]
pub(crate) fn react_language_changed(mut editors: ReactLanguageChangedQuery) {
    for (grammar, mut syntax_state) in editors.iter_mut() {
        if let Some(provider) = grammar.create_provider() {
            syntax_state.provider = Some(provider);
        }
    }
}

#[cfg(feature = "tree-sitter")]
/// Mirror `TextBuffer.rope` + `content_version` into the per-entity
/// `EditorBufferSnapshot` so the next `parse_dirty` tick sees the latest content.
pub(crate) fn sync_editor_parse_source(editors: SyncEditorParseSourceQuery) {
    for (buffer, buf_ref) in editors.iter() {
        let mut buf = buf_ref.0.write().unwrap();
        if buf.content_version == buffer.content_version {
            continue;
        }
        buf.rope = buffer.rope.clone();
        buf.content_version = buffer.content_version;
    }
}

#[cfg(feature = "tree-sitter")]
/// Apply edits synchronously to `SyntaxTree::tree` (tree interpolation).
///
/// `tree.edit()` shifts byte offsets in O(log n) so highlight queries stay
/// valid while the async re-parse runs. Reads `TextEdited` events emitted by
/// the `on_edit_invalidate_caches` observer.
pub(crate) fn record_edits_for_incremental_parsing(
    mut editor_query: Query<&mut bevy_tree_sitter::SyntaxTree, With<CodeEditor>>,
    mut events: MessageReader<crate::types::events::TextEdited>,
) {
    let collected_events: Vec<_> = events.read().cloned().collect();
    for mut syntax_tree in editor_query.iter_mut() {
        let mut forced_full_rebuild = false;
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

            let removed = edit.old_end_byte.saturating_sub(edit.start_byte);
            let inserted = edit.new_end_byte.saturating_sub(edit.start_byte);
            const HUGE_EDIT_THRESHOLD: usize = 64 * 1024;
            let huge_edit = removed > HUGE_EDIT_THRESHOLD || inserted > HUGE_EDIT_THRESHOLD;

            if !huge_edit {
                let st = syntax_tree.bypass_change_detection();
                if let Some(tree) = st.tree.as_mut() {
                    tree.edit(&edit);
                }
                let line_count_changed =
                    edit.old_end_position.row != edit.new_end_position.row;
                if line_count_changed {
                    forced_full_rebuild = true;
                    st.dirty_rows = None;
                } else if !forced_full_rebuild {
                    let start_row = edit.start_position.row as u32;
                    let end_row = edit.new_end_position.row as u32;
                    st.dirty_rows = Some(match st.dirty_rows {
                        Some((lo, hi)) => (lo.min(start_row), hi.max(end_row)),
                        None => (start_row, end_row),
                    });
                }
            } else {
                let st = syntax_tree.bypass_change_detection();
                st.tree = None;
                st.dirty_rows = None;
            }
        }
    }
}

pub struct SyntaxPlugin;

impl Plugin for SyntaxPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<crate::types::events::TextEdited>();

        app.add_systems(Startup, init_editor_syntax);
        app.add_systems(
            Update,
            init_editor_syntax.in_set(crate::plugin::ApplyStateSet),
        );

        #[cfg(feature = "tree-sitter")]
        {
            if !app.is_plugin_added::<bevy_tree_sitter::TreeSitterPlugin>() {
                app.add_plugins(bevy_tree_sitter::TreeSitterPlugin);
            }

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
            // mirror_syntax_tree_to_provider removed: SyntaxTree is the
            // single source of truth; highlight_range reads it directly.
        }
    }
}
