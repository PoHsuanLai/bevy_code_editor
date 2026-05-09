//! Buffer edit handlers — Insert{Newline,Tab}, Delete{Backward,Forward,
//! WordBackward,WordForward,Line}, Undo, Redo.
//!
//! Tab insertion uses a hard `\t` here. Hosts that want "soft tabs" (insert
//! N spaces) should consume `InsertTabRequested` themselves with a more
//! involved handler ahead of this one — or skip emitting the event when
//! their indentation policy is non-default.

use crate::editing_events::*;
use crate::history::EditKind;
use crate::state::{CursorState, EditHistoryState, IndentConfig, SelectionState, TextEditor};
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy_instanced_text::{ContentMetrics, TextBuffer};

type EditorBufQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut SelectionState,
        &'static mut EditHistoryState,
        &'static mut CursorState,
        &'static mut TextBuffer,
    ),
    With<TextEditor>,
>;

type EditorSetTextQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut SelectionState,
        &'static mut EditHistoryState,
        &'static mut CursorState,
        &'static mut TextBuffer,
        &'static mut ContentMetrics,
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
    buffer: &mut TextBuffer,
    c: char,
) {
    if sel.selections.primary().has_selection() {
        delete_selection(sel, hist, cursor, buffer);
    }
    hist.insert_char(sel, cursor, buffer, c);
}

/// Delete the active selection range and record one undo operation.
pub fn delete_selection(
    sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    cursor: &mut CursorState,
    buffer: &mut TextBuffer,
) {
    let Some((start, end)) = sel.primary_range() else {
        return;
    };
    let outcome = hist.replace_range(buffer, start, end, "", EditKind::Other, true);
    cursor.cursor_pos = outcome.new_cursor_pos;
    sel.apply_primary_cursor(cursor);
}

/// System: inserts a newline at the cursor (with optional auto-indent).
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
    let Ok((mut sel, mut hist, mut cursor, mut buffer)) = q.get_mut(entity) else {
        return;
    };
    insert_char(&mut sel, &mut hist, &mut cursor, &mut buffer, '\n');
}

/// System: inserts a tab (or spaces, depending on `IndentConfig`) at the cursor.
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
    let Ok((mut sel, mut hist, mut cursor, mut buffer)) = q.get_mut(entity) else {
        return;
    };
    let indent = indent_q.get(entity).copied().unwrap_or_default();
    if indent.use_spaces {
        for _ in 0..indent.tab_width {
            insert_char(&mut sel, &mut hist, &mut cursor, &mut buffer, ' ');
        }
    } else {
        insert_char(&mut sel, &mut hist, &mut cursor, &mut buffer, '\t');
    }
}

/// System: deletes one character before the cursor (or the selection).
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
    let Ok((mut sel, mut hist, mut cursor, mut buffer)) = q.get_mut(entity) else {
        return;
    };
    let _span = bevy::prelude::info_span!("delete_backward").entered();
    if sel.selections.primary().has_selection() {
        delete_selection(&mut sel, &mut hist, &mut cursor, &mut buffer);
    } else {
        hist.delete_backward(&mut sel, &mut cursor, &mut buffer);
    }
}

/// System: deletes one character after the cursor (or the selection).
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
    let Ok((mut sel, mut hist, mut cursor, mut buffer)) = q.get_mut(entity) else {
        return;
    };
    if sel.selections.primary().has_selection() {
        delete_selection(&mut sel, &mut hist, &mut cursor, &mut buffer);
    } else {
        hist.delete_forward(&mut sel, &mut cursor, &mut buffer);
    }
}

/// System: deletes from the previous word boundary to the cursor (Ctrl+Backspace).
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
    let Ok((mut sel, mut hist, mut cursor, mut buffer)) = q.get_mut(entity) else {
        return;
    };
    if sel.selections.primary().has_selection() {
        delete_selection(&mut sel, &mut hist, &mut cursor, &mut buffer);
    } else {
        delete_word_backward(&mut sel, &mut hist, &mut cursor, &mut buffer);
    }
}

/// System: deletes from the cursor to the next word boundary (Ctrl+Delete).
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
    let Ok((mut sel, mut hist, mut cursor, mut buffer)) = q.get_mut(entity) else {
        return;
    };
    if sel.selections.primary().has_selection() {
        delete_selection(&mut sel, &mut hist, &mut cursor, &mut buffer);
    } else {
        delete_word_forward(&mut sel, &mut hist, &mut cursor, &mut buffer);
    }
}

/// System: deletes the current line (stub — drains the event queue).
pub fn handle_delete_line(mut events: MessageReader<DeleteLineRequested>) {
    // Stub: not implemented in the legacy editor either; drain the queue.
    events.read().for_each(|_| {});
}

/// System: undoes the last edit operation.
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
    let Ok((mut sel, mut hist, mut cursor, mut buffer)) = q.get_mut(entity) else {
        return;
    };
    let _ = hist.undo(&mut sel, &mut cursor, &mut buffer);
}

/// System: redoes the last undone edit operation.
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
    let Ok((mut sel, mut hist, mut cursor, mut buffer)) = q.get_mut(entity) else {
        return;
    };
    let _ = hist.redo(&mut sel, &mut cursor, &mut buffer);
}

/// Delete from word start to cursor.
pub fn delete_word_backward(
    sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    cursor: &mut CursorState,
    buffer: &mut TextBuffer,
) {
    let word_start =
        crate::cursor_movement::find_word_boundary_left(&buffer.rope, cursor.cursor_pos);
    if word_start >= cursor.cursor_pos {
        return;
    }
    let outcome = hist.replace_range(
        buffer,
        word_start,
        cursor.cursor_pos,
        "",
        EditKind::DeleteBackward,
        true,
    );
    cursor.cursor_pos = outcome.new_cursor_pos;
    sel.apply_primary_cursor(cursor);
}

/// Delete from cursor to word end.
pub fn delete_word_forward(
    sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    cursor: &mut CursorState,
    buffer: &mut TextBuffer,
) {
    let word_end =
        crate::cursor_movement::find_word_boundary_right(&buffer.rope, cursor.cursor_pos);
    if word_end <= cursor.cursor_pos {
        return;
    }
    hist.replace_range(
        buffer,
        cursor.cursor_pos,
        word_end,
        "",
        EditKind::DeleteForward,
        true,
    );
    sel.apply_primary_cursor(cursor);
}

/// Apply `ReplaceRangeRequested` events. Routes to the requested entity and
/// uses `replace_range` to mutate, ensuring history + anchors + `OnEdit`
/// stay correct. Cross-crate consumers (LSP, completion, formatting,
/// refactoring) emit the event and never touch editor state directly.
pub fn handle_replace_range(
    mut events: MessageReader<ReplaceRangeRequested>,
    mut q: EditorBufQuery,
) {
    for event in events.read() {
        let Ok((mut sel, mut hist, mut cursor, mut buffer)) = q.get_mut(event.entity) else {
            continue;
        };
        let outcome = hist.replace_range(
            &mut buffer,
            event.start_char,
            event.end_char,
            &event.text,
            event.kind,
            event.record_history,
        );
        if cursor.cursor_pos >= event.start_char {
            cursor.cursor_pos = outcome.new_cursor_pos;
            sel.apply_primary_cursor(&cursor);
        }
    }
}

/// Apply `SetTextRequested` events. Replaces the whole buffer.
pub fn handle_set_text(mut events: MessageReader<SetTextRequested>, mut q: EditorSetTextQuery) {
    for event in events.read() {
        let Ok((mut sel, mut hist, mut cursor, mut buffer, mut metrics)) = q.get_mut(event.entity)
        else {
            continue;
        };
        hist.set_text(
            &mut sel,
            &mut cursor,
            &mut buffer,
            &mut metrics,
            &event.text,
        );
    }
}
