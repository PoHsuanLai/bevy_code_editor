//! Clipboard handlers — Copy / Cut / Paste.

use crate::editing_events::*;
use crate::history::{EditKind, EditOperation};
use crate::state::{CursorState, EditHistoryState, SelectionState, TextEditor};
use arboard::Clipboard;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy_text_engine::TextViewState;

pub fn handle_copy(
    mut events: MessageReader<CopyRequested>,
    input_focus: Res<InputFocus>,
    q: Query<(&SelectionState, &TextViewState), With<TextEditor>>,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((sel, tv)) = q.get(entity) else {
        return;
    };
    if let Some((start, end)) = sel.primary_range() {
        let start = start.min(tv.rope.len_chars());
        let end = end.min(tv.rope.len_chars());
        let text = tv.rope.slice(start..end).to_string();
        if let Ok(mut clipboard) = Clipboard::new() {
            let _ = clipboard.set_text(text);
        }
    }
}

pub fn handle_cut(
    mut events: MessageReader<CutRequested>,
    input_focus: Res<InputFocus>,
    mut q: Query<
        (
            &mut SelectionState,
            &mut EditHistoryState,
            &mut CursorState,
            &mut TextViewState,
        ),
        With<TextEditor>,
    >,
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
    let Some((start, end)) = sel.primary_range() else {
        return;
    };
    let start = start.min(tv.rope.len_chars());
    let end = end.min(tv.rope.len_chars());
    let selected_text = tv.rope.slice(start..end).to_string();
    let cursor_before = cursor.cursor_pos;

    if let Ok(mut clipboard) = Clipboard::new() {
        let _ = clipboard.set_text(selected_text.clone());
    }

    let start_byte = tv.rope.char_to_byte(start);
    let end_byte = tv.rope.char_to_byte(end);
    let start_position = crate::edit::point_at_byte(&tv.rope, start_byte);
    let old_end_position = crate::edit::point_at_byte(&tv.rope, end_byte);
    tv.rope.remove(start_byte..end_byte);
    cursor.cursor_pos = start;

    hist.history.record(EditOperation {
        removed_text: selected_text,
        inserted_text: String::new(),
        position: start,
        cursor_before,
        cursor_after: start,
        kind: EditKind::Other,
    });
    hist.pending_byte_edit = Some(crate::EditDelta {
        start_byte,
        old_end_byte: end_byte,
        new_end_byte: start_byte,
        start_position,
        old_end_position,
        new_end_position: start_position,
    });

    sel.apply_primary_cursor(&cursor);
    tv.content_version += 1;
}

pub fn handle_paste(
    mut events: MessageReader<PasteRequested>,
    input_focus: Res<InputFocus>,
    mut q: Query<
        (
            &mut SelectionState,
            &mut EditHistoryState,
            &mut CursorState,
            &mut TextViewState,
        ),
        With<TextEditor>,
    >,
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
    let Ok(mut clipboard) = Clipboard::new() else {
        return;
    };
    let Ok(text) = clipboard.get_text() else {
        return;
    };

    let cursor_before = cursor.cursor_pos;
    let mut deleted_text = String::new();
    let paste_position;
    let removed_bytes;
    let start_byte_pre;
    let start_position;
    let old_end_position;

    if let Some((start, end)) = sel.primary_range() {
        let start = start.min(tv.rope.len_chars());
        let end = end.min(tv.rope.len_chars());
        deleted_text = tv.rope.slice(start..end).to_string();
        let s_byte = tv.rope.char_to_byte(start);
        let e_byte = tv.rope.char_to_byte(end);
        start_byte_pre = s_byte;
        start_position = crate::edit::point_at_byte(&tv.rope, s_byte);
        old_end_position = crate::edit::point_at_byte(&tv.rope, e_byte);
        removed_bytes = e_byte - s_byte;
        tv.rope.remove(s_byte..e_byte);
        cursor.cursor_pos = start;
        paste_position = start;
    } else {
        paste_position = cursor.cursor_pos.min(tv.rope.len_chars());
        let s_byte = tv.rope.char_to_byte(paste_position);
        start_byte_pre = s_byte;
        start_position = crate::edit::point_at_byte(&tv.rope, s_byte);
        old_end_position = start_position;
        removed_bytes = 0;
    }

    let had_newlines = text.contains('\n') || deleted_text.contains('\n');
    tv.rope.insert(paste_position, &text);
    cursor.cursor_pos = paste_position + text.chars().count();
    sel.apply_primary_cursor(&cursor);
    tv.content_version += 1;

    let new_end_byte = start_byte_pre + text.len();
    hist.history.record(EditOperation {
        removed_text: deleted_text,
        inserted_text: text.clone(),
        position: paste_position,
        cursor_before,
        cursor_after: cursor.cursor_pos,
        kind: EditKind::Paste,
    });
    hist.pending_byte_edit = Some(crate::EditDelta {
        start_byte: start_byte_pre,
        old_end_byte: start_byte_pre + removed_bytes,
        new_end_byte,
        start_position,
        old_end_position,
        new_end_position: crate::edit::point_at_byte(&tv.rope, new_end_byte),
    });

    if had_newlines {
        let line_idx = tv.rope.char_to_line(paste_position);
        hist.invalidate_lines_from = Some(line_idx);
    }
}
