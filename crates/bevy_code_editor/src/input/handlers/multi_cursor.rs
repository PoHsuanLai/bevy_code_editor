//! Multi-cursor handlers — AddCursor{AtNextOccurrence,Above,Below},
//! ClearSecondaryCursors.

use crate::input::action_events::*;
use crate::input::editor_ops::add_cursor_at_next_occurrence;
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

pub fn handle_add_cursor_at_next_occurrence(
    mut events: MessageReader<AddCursorAtNextOccurrenceRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut cursor, mut tv)) = q.get_mut(entity) else {
        return;
    };
    sel.sync_cursors_from_primary(&mut cursor);
    add_cursor_at_next_occurrence(&mut sel, &mut cursor, &mut tv);
}

pub fn handle_add_cursor_above(
    mut events: MessageReader<AddCursorAboveRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut cursor, mut tv)) = q.get_mut(entity) else {
        return;
    };
    sel.sync_cursors_from_primary(&mut cursor);
    add_cursor_above(&mut sel, &mut cursor, &mut tv);
}

pub fn handle_add_cursor_below(
    mut events: MessageReader<AddCursorBelowRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut cursor, mut tv)) = q.get_mut(entity) else {
        return;
    };
    sel.sync_cursors_from_primary(&mut cursor);
    add_cursor_below(&mut sel, &mut cursor, &mut tv);
}

pub fn handle_clear_secondary_cursors(
    mut events: MessageReader<ClearSecondaryCursorsRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorView,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut cursor, mut tv)) = q.get_mut(entity) else {
        return;
    };
    if sel.has_multiple_cursors(&cursor) {
        sel.clear_secondary_cursors(&mut cursor, &mut tv);
    }
}

/// Add a cursor on the line above the primary cursor.
fn add_cursor_above(
    sel: &mut SelectionState,
    cursor: &mut CursorState,
    tv: &mut crate::text_view::TextViewState,
) {
    if cursor.cursors.is_empty() {
        return;
    }

    let primary_pos = cursor.cursors[0].position;
    let line_idx = tv.rope.char_to_line(primary_pos);

    if line_idx == 0 {
        return;
    }

    let line_start = tv.rope.line_to_char(line_idx);
    let col_offset = primary_pos - line_start;

    let prev_line_start = tv.rope.line_to_char(line_idx - 1);
    let prev_line_len = tv.rope.line(line_idx - 1).len_chars().saturating_sub(1);
    let new_pos = prev_line_start + col_offset.min(prev_line_len);

    sel.add_cursor(cursor, tv, new_pos);
}

/// Add a cursor on the line below the primary cursor.
fn add_cursor_below(
    sel: &mut SelectionState,
    cursor: &mut CursorState,
    tv: &mut crate::text_view::TextViewState,
) {
    if cursor.cursors.is_empty() {
        return;
    }

    let primary_pos = cursor.cursors[0].position;
    let line_idx = tv.rope.char_to_line(primary_pos);

    if line_idx + 1 >= tv.rope.len_lines() {
        return;
    }

    let line_start = tv.rope.line_to_char(line_idx);
    let col_offset = primary_pos - line_start;

    let next_line_start = tv.rope.line_to_char(line_idx + 1);
    let next_line_len = tv.rope.line(line_idx + 1).len_chars().saturating_sub(1);
    let new_pos = next_line_start + col_offset.min(next_line_len);

    sel.add_cursor(cursor, tv, new_pos);
}
