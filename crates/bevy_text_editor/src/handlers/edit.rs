//! Buffer edit handlers — Insert{Newline,Tab}, Delete{Backward,Forward,
//! WordBackward,WordForward,Line}, Undo, Redo.
//!
//! Tab insertion uses a hard `\t` here. Hosts that want "soft tabs" (insert
//! N spaces) should consume `InsertTabRequested` themselves with a more
//! involved handler ahead of this one — or skip emitting the event when
//! their indentation policy is non-default.

use crate::editing_events::*;
use crate::history::{EditKind, EditOperation};
use crate::state::{CursorState, EditHistoryState, IndentConfig, SelectionState, TextEditor};
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy_text_engine::TextViewState;

type EditorBufQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut SelectionState,
        &'static mut EditHistoryState,
        &'static mut CursorState,
        &'static mut TextViewState,
    ),
    With<TextEditor>,
>;

/// Insert a character at cursor — drops any active selection first, then
/// records the insert in history. Available for hosts that want to type a
/// specific character programmatically.
pub fn insert_char(
    sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    cursor: &mut CursorState,
    tv: &mut TextViewState,
    c: char,
) {
    if sel.selections.primary().has_selection() {
        delete_selection(sel, hist, cursor, tv);
    }
    hist.insert_char(sel, cursor, tv, c);
}

/// Delete the active selection range and record one undo operation.
pub fn delete_selection(
    sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    cursor: &mut CursorState,
    tv: &mut TextViewState,
) {
    let Some((start, end)) = sel.primary_range() else {
        return;
    };
    let cursor_before = cursor.cursor_pos;
    let deleted_text: String = tv.rope.slice(start..end).chars().collect();

    let start_byte = tv.rope.char_to_byte(start);
    let end_byte = tv.rope.char_to_byte(end);
    let start_position = crate::edit::point_at_byte(&tv.rope, start_byte);
    let old_end_position = crate::edit::point_at_byte(&tv.rope, end_byte);

    tv.rope.remove(start_byte..end_byte);
    cursor.cursor_pos = start;

    if !deleted_text.is_empty() {
        hist.history.record(EditOperation {
            removed_text: deleted_text,
            inserted_text: String::new(),
            position: start,
            cursor_before,
            cursor_after: start,
            kind: EditKind::Other,
        });
    }

    hist.pending_byte_edit = Some(crate::EditDelta {
        start_byte,
        old_end_byte: end_byte,
        new_end_byte: start_byte,
        start_position,
        old_end_position,
        new_end_position: start_position,
    });

    sel.apply_primary_cursor(cursor);
    tv.content_version += 1;
}

pub fn handle_insert_newline(
    mut events: MessageReader<InsertNewlineRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorBufQuery,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut hist, mut cursor, mut tv)) = q.get_mut(entity) else {
        return;
    };
    insert_char(&mut sel, &mut hist, &mut cursor, &mut tv, '\n');
}

pub fn handle_insert_tab(
    mut events: MessageReader<InsertTabRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorBufQuery,
    indent_q: Query<&IndentConfig, With<TextEditor>>,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut hist, mut cursor, mut tv)) = q.get_mut(entity) else {
        return;
    };
    let indent = indent_q.get(entity).copied().unwrap_or_default();
    if indent.use_spaces {
        for _ in 0..indent.tab_width {
            insert_char(&mut sel, &mut hist, &mut cursor, &mut tv, ' ');
        }
    } else {
        insert_char(&mut sel, &mut hist, &mut cursor, &mut tv, '\t');
    }
}

pub fn handle_delete_backward(
    mut events: MessageReader<DeleteBackwardRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorBufQuery,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut hist, mut cursor, mut tv)) = q.get_mut(entity) else {
        return;
    };
    if sel.selections.primary().has_selection() {
        delete_selection(&mut sel, &mut hist, &mut cursor, &mut tv);
    } else {
        hist.delete_backward(&mut sel, &mut cursor, &mut tv);
    }
}

pub fn handle_delete_forward(
    mut events: MessageReader<DeleteForwardRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorBufQuery,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut hist, mut cursor, mut tv)) = q.get_mut(entity) else {
        return;
    };
    if sel.selections.primary().has_selection() {
        delete_selection(&mut sel, &mut hist, &mut cursor, &mut tv);
    } else {
        hist.delete_forward(&mut sel, &mut cursor, &mut tv);
    }
}

pub fn handle_delete_word_backward(
    mut events: MessageReader<DeleteWordBackwardRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorBufQuery,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut hist, mut cursor, mut tv)) = q.get_mut(entity) else {
        return;
    };
    if sel.selections.primary().has_selection() {
        delete_selection(&mut sel, &mut hist, &mut cursor, &mut tv);
    } else {
        delete_word_backward(&mut sel, &mut hist, &mut cursor, &mut tv);
    }
}

pub fn handle_delete_word_forward(
    mut events: MessageReader<DeleteWordForwardRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorBufQuery,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut hist, mut cursor, mut tv)) = q.get_mut(entity) else {
        return;
    };
    if sel.selections.primary().has_selection() {
        delete_selection(&mut sel, &mut hist, &mut cursor, &mut tv);
    } else {
        delete_word_forward(&mut sel, &mut hist, &mut cursor, &mut tv);
    }
}

pub fn handle_delete_line(mut events: MessageReader<DeleteLineRequested>) {
    // Stub: not implemented in the legacy editor either; drain the queue.
    events.read().for_each(|_| {});
}

pub fn handle_undo(
    mut events: MessageReader<UndoRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorBufQuery,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut hist, mut cursor, mut tv)) = q.get_mut(entity) else {
        return;
    };
    let _ = hist.undo(&mut sel, &mut cursor, &mut tv);
}

pub fn handle_redo(
    mut events: MessageReader<RedoRequested>,
    input_focus: Res<InputFocus>,
    mut q: EditorBufQuery,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((mut sel, mut hist, mut cursor, mut tv)) = q.get_mut(entity) else {
        return;
    };
    let _ = hist.redo(&mut sel, &mut cursor, &mut tv);
}

/// Delete from word start to cursor.
pub fn delete_word_backward(
    sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    cursor: &mut CursorState,
    tv: &mut TextViewState,
) {
    let cursor_before = cursor.cursor_pos;
    let word_start = crate::cursor_movement::find_word_boundary_left(&tv.rope, cursor.cursor_pos);

    if word_start < cursor_before {
        let deleted_text: String = tv.rope.slice(word_start..cursor_before).chars().collect();

        let start_byte = tv.rope.char_to_byte(word_start);
        let end_byte = tv.rope.char_to_byte(cursor_before);
        let start_position = crate::edit::point_at_byte(&tv.rope, start_byte);
        let old_end_position = crate::edit::point_at_byte(&tv.rope, end_byte);

        tv.rope.remove(start_byte..end_byte);

        cursor.cursor_pos = word_start;
        sel.apply_primary_cursor(cursor);

        hist.history.record(EditOperation {
            removed_text: deleted_text,
            inserted_text: String::new(),
            position: word_start,
            cursor_before,
            cursor_after: word_start,
            kind: EditKind::DeleteBackward,
        });
        hist.pending_byte_edit = Some(crate::EditDelta {
            start_byte,
            old_end_byte: end_byte,
            new_end_byte: start_byte,
            start_position,
            old_end_position,
            new_end_position: start_position,
        });

        tv.content_version += 1;
    }
}

/// Delete from cursor to word end.
pub fn delete_word_forward(
    sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    cursor: &mut CursorState,
    tv: &mut TextViewState,
) {
    let cursor_before = cursor.cursor_pos;
    let word_end = crate::cursor_movement::find_word_boundary_right(&tv.rope, cursor.cursor_pos);

    if word_end > cursor_before {
        let deleted_text: String = tv.rope.slice(cursor_before..word_end).chars().collect();

        let start_byte = tv.rope.char_to_byte(cursor_before);
        let end_byte = tv.rope.char_to_byte(word_end);
        let start_position = crate::edit::point_at_byte(&tv.rope, start_byte);
        let old_end_position = crate::edit::point_at_byte(&tv.rope, end_byte);

        tv.rope.remove(start_byte..end_byte);

        sel.apply_primary_cursor(cursor);

        hist.history.record(EditOperation {
            removed_text: deleted_text,
            inserted_text: String::new(),
            position: cursor_before,
            cursor_before,
            cursor_after: cursor_before,
            kind: EditKind::DeleteForward,
        });
        hist.pending_byte_edit = Some(crate::EditDelta {
            start_byte,
            old_end_byte: end_byte,
            new_end_byte: start_byte,
            start_position,
            old_end_position,
            new_end_position: start_position,
        });

        tv.content_version += 1;
    }
}
