//! Editor-side side-effect drain for [`bevy_text_editor`] edits.
//!
//! The editable-text core (insert / delete / undo / redo / set_text) lives
//! in `bevy_text_editor`. After every edit op, `EditHistoryState` exposes
//! two side-channels:
//! - `pending_byte_edit` — `(start_byte, old_end_byte, new_end_byte)` for
//!   incremental tree-sitter reparse.
//! - `invalidate_lines_from` — the line index from which line-keyed entities
//!   need re-spawning.
//!
//! The [`drain_edit_side_effects`] system runs every Update after the
//! editing handlers, copying these into the editor's `SyntaxCacheState` and
//! `EditorDisplayState`. Hosts that mutate edit state imperatively from
//! within an observer (e.g. the bracket auto-close path in
//! [`crate::input::keyboard`]) call [`drain_one`] directly to propagate the
//! side-channel within the same handler invocation.

use crate::text_view::TextViewState;
use crate::types::{EditHistoryState, EditorDisplayState, SyntaxCacheState};
use bevy::prelude::*;

/// Drain `EditHistoryState`'s side-channel fields onto the editor's caches.
/// Idempotent — taking the field clears it.
pub fn drain_one(
    hist: &mut EditHistoryState,
    syntax: &mut SyntaxCacheState,
    display: &mut EditorDisplayState,
) {
    if let Some(byte_edit) = hist.pending_byte_edit.take() {
        #[cfg(feature = "tree-sitter")]
        {
            syntax.pending_tree_sitter_edit = Some(byte_edit);
        }
        #[cfg(not(feature = "tree-sitter"))]
        {
            let _ = byte_edit;
            let _ = syntax;
        }
    }
    if let Some(line_idx) = hist.invalidate_lines_from.take() {
        display.invalidate_lines_from = Some(line_idx);
    }
}

/// Drain side-effects across every editor entity each Update.
pub fn drain_edit_side_effects(
    mut q: Query<
        (&mut EditHistoryState, &mut SyntaxCacheState, &mut EditorDisplayState),
        With<crate::types::CodeEditor>,
    >,
) {
    for (mut hist, mut syntax, mut display) in q.iter_mut() {
        if hist.pending_byte_edit.is_some() || hist.invalidate_lines_from.is_some() {
            drain_one(&mut hist, &mut syntax, &mut display);
        }
    }
}

/// Set buffer text + drain side effects eagerly.
///
/// Wraps [`bevy_text_editor::EditHistoryState::set_text`] so callers that
/// load a file at startup (before [`drain_edit_side_effects`] runs) still
/// get the syntax/display caches updated for the initial render.
#[allow(clippy::too_many_arguments)]
pub fn set_text(
    sel: &mut crate::types::SelectionState,
    hist: &mut EditHistoryState,
    syntax: &mut SyntaxCacheState,
    display: &mut EditorDisplayState,
    cursor: &mut crate::types::CursorState,
    tv: &mut TextViewState,
    text: &str,
) {
    hist.set_text(sel, cursor, tv, text);
    drain_one(hist, syntax, display);
}
