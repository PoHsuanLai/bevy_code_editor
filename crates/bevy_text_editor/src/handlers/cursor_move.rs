//! Cursor movement handlers — Move{Left,Right,Up,Down,Word*,LineStart,LineEnd,
//! DocumentStart,DocumentEnd,PageUp,PageDown}.

use crate::cursor_movement::{
    move_cursor, move_cursor_down, move_cursor_line_end, move_cursor_line_start, move_cursor_up,
    move_cursor_word_left, move_cursor_word_right,
};
use crate::editing_events::*;
use crate::state::{CursorState, SelectionState, TextEditor};
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy_text_engine::TextViewState;

type EditorView<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut SelectionState,
        &'static mut CursorState,
        &'static mut TextViewState,
    ),
    With<TextEditor>,
>;

fn focused(input_focus: &InputFocus) -> Option<Entity> {
    input_focus.get()
}

pub fn handle_move_cursor_left(
    mut events: MessageReader<MoveCursorLeftRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = focused(&input_focus) else {
        return;
    };
    let Ok((mut sel, mut cursor, tv)) = q.get_mut(entity) else {
        return;
    };
    move_cursor(&mut cursor, &tv.rope, -1);
    sel.apply_primary_cursor(&cursor);
}

pub fn handle_move_cursor_right(
    mut events: MessageReader<MoveCursorRightRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = focused(&input_focus) else {
        return;
    };
    let Ok((mut sel, mut cursor, tv)) = q.get_mut(entity) else {
        return;
    };
    move_cursor(&mut cursor, &tv.rope, 1);
    sel.apply_primary_cursor(&cursor);
}

pub fn handle_move_cursor_up(
    mut events: MessageReader<MoveCursorUpRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = focused(&input_focus) else {
        return;
    };
    let Ok((mut sel, mut cursor, tv)) = q.get_mut(entity) else {
        return;
    };
    move_cursor_up(&mut cursor, &tv.rope);
    sel.apply_primary_cursor(&cursor);
}

pub fn handle_move_cursor_down(
    mut events: MessageReader<MoveCursorDownRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = focused(&input_focus) else {
        return;
    };
    let Ok((mut sel, mut cursor, tv)) = q.get_mut(entity) else {
        return;
    };
    move_cursor_down(&mut cursor, &tv.rope);
    sel.apply_primary_cursor(&cursor);
}

pub fn handle_move_cursor_word_left(
    mut events: MessageReader<MoveCursorWordLeftRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = focused(&input_focus) else {
        return;
    };
    let Ok((mut sel, mut cursor, tv)) = q.get_mut(entity) else {
        return;
    };
    move_cursor_word_left(&mut cursor, &tv.rope);
    sel.apply_primary_cursor(&cursor);
}

pub fn handle_move_cursor_word_right(
    mut events: MessageReader<MoveCursorWordRightRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = focused(&input_focus) else {
        return;
    };
    let Ok((mut sel, mut cursor, tv)) = q.get_mut(entity) else {
        return;
    };
    move_cursor_word_right(&mut cursor, &tv.rope);
    sel.apply_primary_cursor(&cursor);
}

pub fn handle_move_cursor_line_start(
    mut events: MessageReader<MoveCursorLineStartRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = focused(&input_focus) else {
        return;
    };
    let Ok((mut sel, mut cursor, tv)) = q.get_mut(entity) else {
        return;
    };
    move_cursor_line_start(&mut cursor, &tv.rope);
    sel.apply_primary_cursor(&cursor);
}

pub fn handle_move_cursor_line_end(
    mut events: MessageReader<MoveCursorLineEndRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = focused(&input_focus) else {
        return;
    };
    let Ok((mut sel, mut cursor, tv)) = q.get_mut(entity) else {
        return;
    };
    move_cursor_line_end(&mut cursor, &tv.rope);
    sel.apply_primary_cursor(&cursor);
}

pub fn handle_move_cursor_document_start(
    mut events: MessageReader<MoveCursorDocumentStartRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = focused(&input_focus) else {
        return;
    };
    let Ok((mut sel, mut cursor, _tv)) = q.get_mut(entity) else {
        return;
    };
    cursor.cursor_pos = 0;
    sel.apply_primary_cursor(&cursor);
}

pub fn handle_move_cursor_document_end(
    mut events: MessageReader<MoveCursorDocumentEndRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = focused(&input_focus) else {
        return;
    };
    let Ok((mut sel, mut cursor, tv)) = q.get_mut(entity) else {
        return;
    };
    cursor.cursor_pos = tv.rope.len_chars();
    sel.apply_primary_cursor(&cursor);
}

pub fn handle_move_cursor_page_up(
    mut events: MessageReader<MoveCursorPageUpRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = focused(&input_focus) else {
        return;
    };
    let Ok((mut sel, cursor, _tv)) = q.get_mut(entity) else {
        return;
    };
    // TODO: page-up was a TODO pre-refactor too.
    sel.apply_primary_cursor(&cursor);
}

pub fn handle_move_cursor_page_down(
    mut events: MessageReader<MoveCursorPageDownRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = focused(&input_focus) else {
        return;
    };
    let Ok((mut sel, cursor, _tv)) = q.get_mut(entity) else {
        return;
    };
    // TODO: page-down was a TODO pre-refactor too.
    sel.apply_primary_cursor(&cursor);
}
