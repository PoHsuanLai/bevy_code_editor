//! Editor-side wrappers + side-effect drain for [`bevy_text_editor`] edits.
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
//! `EditorDisplayState`. The wrappers below also drain the side-channels
//! eagerly for handlers / observers that mutate edit state imperatively.

use crate::text_view::TextViewState;
use crate::types::{
    CursorState, EditHistoryState, EditorDisplayState, SelectionState, SyntaxCacheState,
};
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

/// Insert a character with bevy_code_editor side-effect propagation.
#[allow(clippy::too_many_arguments)]
pub fn insert_char(
    sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    syntax: &mut SyntaxCacheState,
    display: &mut EditorDisplayState,
    cursor: &mut CursorState,
    tv: &mut TextViewState,
    c: char,
) {
    if sel.selections.primary().has_selection() {
        delete_selection(sel, hist, syntax, display, cursor, tv);
    }
    hist.insert_char(sel, cursor, tv, c);
    drain_one(hist, syntax, display);
}

/// Delete the active selection range (with side-effect propagation).
pub fn delete_selection(
    sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    syntax: &mut SyntaxCacheState,
    display: &mut EditorDisplayState,
    cursor: &mut CursorState,
    tv: &mut TextViewState,
) {
    bevy_text_editor::handlers::edit::delete_selection(sel, hist, cursor, tv);
    drain_one(hist, syntax, display);
}

#[allow(clippy::too_many_arguments)]
pub fn delete_backward(
    sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    syntax: &mut SyntaxCacheState,
    display: &mut EditorDisplayState,
    cursor: &mut CursorState,
    tv: &mut TextViewState,
) {
    hist.delete_backward(sel, cursor, tv);
    drain_one(hist, syntax, display);
}

#[allow(clippy::too_many_arguments)]
pub fn delete_forward(
    sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    syntax: &mut SyntaxCacheState,
    display: &mut EditorDisplayState,
    cursor: &mut CursorState,
    tv: &mut TextViewState,
) {
    hist.delete_forward(sel, cursor, tv);
    drain_one(hist, syntax, display);
}

#[allow(clippy::too_many_arguments)]
pub fn undo(
    sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    syntax: &mut SyntaxCacheState,
    display: &mut EditorDisplayState,
    cursor: &mut CursorState,
    tv: &mut TextViewState,
) -> bool {
    let r = hist.undo(sel, cursor, tv);
    drain_one(hist, syntax, display);
    r
}

#[allow(clippy::too_many_arguments)]
pub fn redo(
    sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    syntax: &mut SyntaxCacheState,
    display: &mut EditorDisplayState,
    cursor: &mut CursorState,
    tv: &mut TextViewState,
) -> bool {
    let r = hist.redo(sel, cursor, tv);
    drain_one(hist, syntax, display);
    r
}

#[allow(clippy::too_many_arguments)]
pub fn delete_word_backward(
    sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    syntax: &mut SyntaxCacheState,
    display: &mut EditorDisplayState,
    cursor: &mut CursorState,
    tv: &mut TextViewState,
) {
    bevy_text_editor::handlers::edit::delete_word_backward(sel, hist, cursor, tv);
    drain_one(hist, syntax, display);
}

#[allow(clippy::too_many_arguments)]
pub fn delete_word_forward(
    sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    syntax: &mut SyntaxCacheState,
    display: &mut EditorDisplayState,
    cursor: &mut CursorState,
    tv: &mut TextViewState,
) {
    bevy_text_editor::handlers::edit::delete_word_forward(sel, hist, cursor, tv);
    drain_one(hist, syntax, display);
}

#[allow(clippy::too_many_arguments)]
pub fn set_text(
    sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    syntax: &mut SyntaxCacheState,
    display: &mut EditorDisplayState,
    cursor: &mut CursorState,
    tv: &mut TextViewState,
    text: &str,
) {
    hist.set_text(sel, cursor, tv, text);
    drain_one(hist, syntax, display);
}
