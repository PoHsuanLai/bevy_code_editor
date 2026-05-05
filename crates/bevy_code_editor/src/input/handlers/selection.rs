//! Selection handlers — Select{Left,Right,Up,Down,WordLeft,WordRight,
//! LineStart,LineEnd,All}, ClearSelection.
//!
//! Selection handlers initialize a selection anchor at the current cursor
//! (if not already selecting), advance the cursor, then update
//! `selection_end` to the new cursor position.

use crate::input::action_events::*;
use crate::input::cursor::{
    init_selection, move_cursor_down, move_cursor_line_end, move_cursor_line_start,
    move_cursor_up, move_cursor_word_left, move_cursor_word_right,
};
use crate::input::editor_ops::move_cursor;
use crate::types::*;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;

type EditorView<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut SelectionState,
        &'static mut CursorState,
        &'static mut crate::text_view::TextViewState,
    ),
    With<CodeEditor>,
>;

pub fn handle_select_left(
    mut events: MessageReader<SelectLeftRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut cursor, tv)) = q.get_mut(entity) else {
        return;
    };
    init_selection(&mut sel, &cursor);
    move_cursor(&mut cursor, &tv.rope, -1);
    sel.selection_end = Some(cursor.cursor_pos);
    sel.sync_cursors_from_primary(&mut cursor);
}

pub fn handle_select_right(
    mut events: MessageReader<SelectRightRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut cursor, tv)) = q.get_mut(entity) else {
        return;
    };
    init_selection(&mut sel, &cursor);
    move_cursor(&mut cursor, &tv.rope, 1);
    sel.selection_end = Some(cursor.cursor_pos);
    sel.sync_cursors_from_primary(&mut cursor);
}

pub fn handle_select_up(
    mut events: MessageReader<SelectUpRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut cursor, tv)) = q.get_mut(entity) else {
        return;
    };
    init_selection(&mut sel, &cursor);
    move_cursor_up(&mut cursor, &tv.rope);
    sel.selection_end = Some(cursor.cursor_pos);
    sel.sync_cursors_from_primary(&mut cursor);
}

pub fn handle_select_down(
    mut events: MessageReader<SelectDownRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut cursor, tv)) = q.get_mut(entity) else {
        return;
    };
    init_selection(&mut sel, &cursor);
    move_cursor_down(&mut cursor, &tv.rope);
    sel.selection_end = Some(cursor.cursor_pos);
    sel.sync_cursors_from_primary(&mut cursor);
}

pub fn handle_select_word_left(
    mut events: MessageReader<SelectWordLeftRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut cursor, tv)) = q.get_mut(entity) else {
        return;
    };
    init_selection(&mut sel, &cursor);
    move_cursor_word_left(&mut cursor, &tv.rope);
    sel.selection_end = Some(cursor.cursor_pos);
    sel.sync_cursors_from_primary(&mut cursor);
}

pub fn handle_select_word_right(
    mut events: MessageReader<SelectWordRightRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut cursor, tv)) = q.get_mut(entity) else {
        return;
    };
    init_selection(&mut sel, &cursor);
    move_cursor_word_right(&mut cursor, &tv.rope);
    sel.selection_end = Some(cursor.cursor_pos);
    sel.sync_cursors_from_primary(&mut cursor);
}

pub fn handle_select_line_start(
    mut events: MessageReader<SelectLineStartRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut cursor, tv)) = q.get_mut(entity) else {
        return;
    };
    init_selection(&mut sel, &cursor);
    move_cursor_line_start(&mut cursor, &tv.rope);
    sel.selection_end = Some(cursor.cursor_pos);
    sel.sync_cursors_from_primary(&mut cursor);
}

pub fn handle_select_line_end(
    mut events: MessageReader<SelectLineEndRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut cursor, tv)) = q.get_mut(entity) else {
        return;
    };
    init_selection(&mut sel, &cursor);
    move_cursor_line_end(&mut cursor, &tv.rope);
    sel.selection_end = Some(cursor.cursor_pos);
    sel.sync_cursors_from_primary(&mut cursor);
}

pub fn handle_select_all(
    mut events: MessageReader<SelectAllRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut cursor, tv)) = q.get_mut(entity) else {
        return;
    };
    sel.selection_start = Some(0);
    sel.selection_end = Some(tv.rope.len_chars());
    cursor.cursor_pos = tv.rope.len_chars();
    sel.sync_cursors_from_primary(&mut cursor);
}

/// Clear-selection has three conditional behaviors that match the original
/// `execute_action` early-return chain:
///   1. If the LSP completion popup is visible, hide it (highest priority).
///   2. Else if multiple cursors are active, drop secondary cursors.
///   3. Else if a goto-line dialog is active, close it.
///   4. Otherwise, clear the active selection range.
pub fn handle_clear_selection(
    mut events: MessageReader<ClearSelectionRequested>,
    input_focus: Res<InputFocus>,
    mut editor_q: Query<
        (
            &mut SelectionState,
            &mut CursorState,
            &mut crate::text_view::TextViewState,
            &mut GotoLineState,
        ),
        With<CodeEditor>,
    >,
    #[cfg(feature = "lsp")] mut lsp_q: Query<
        &mut crate::lsp_ui::state::LspCompletionPopup,
        With<CodeEditor>,
    >,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut cursor, mut tv, mut goto_line_state)) = editor_q.get_mut(entity) else {
        return;
    };

    #[cfg(feature = "lsp")]
    if let Ok(mut completion_state) = lsp_q.get_mut(entity) {
        if completion_state.visible {
            completion_state.visible = false;
            completion_state.filter.clear();
            completion_state.scroll_offset = 0;
            return;
        }
    }

    if sel.has_multiple_cursors(&cursor) {
        sel.clear_secondary_cursors(&mut cursor, &mut tv);
        return;
    }

    if goto_line_state.active {
        goto_line_state.clear();
        return;
    }

    sel.selection_start = None;
    sel.selection_end = None;
    sel.sync_cursors_from_primary(&mut cursor);
}
