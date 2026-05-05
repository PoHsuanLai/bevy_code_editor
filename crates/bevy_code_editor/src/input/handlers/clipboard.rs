//! Clipboard handlers — Copy / Cut / Paste.
//!
//! These read selection state from the focused editor and round-trip the
//! buffer through the system clipboard via `arboard`. Cut and Paste both
//! mutate the rope and record a single edit operation for undo.

use crate::input::action_events::*;
use crate::types::*;
use arboard::Clipboard;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;

pub fn handle_copy(
    mut events: MessageReader<CopyRequested>,
    input_focus: Res<InputFocus>,
    q: Query<
        (
            &SelectionState,
            &crate::text_view::TextViewState,
        ),
        With<CodeEditor>,
    >,
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
            &mut crate::text_view::TextViewState,
        ),
        With<CodeEditor>,
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
            &mut crate::text_view::TextViewState,
        ),
        With<CodeEditor>,
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

    if let Some((start, end)) = sel.primary_range() {
        let start = start.min(tv.rope.len_chars());
        let end = end.min(tv.rope.len_chars());
        deleted_text = tv.rope.slice(start..end).to_string();
        let start_byte = tv.rope.char_to_byte(start);
        let end_byte = tv.rope.char_to_byte(end);
        tv.rope.remove(start_byte..end_byte);
        cursor.cursor_pos = start;
        paste_position = start;
    } else {
        paste_position = cursor.cursor_pos.min(tv.rope.len_chars());
    }

    tv.rope.insert(paste_position, &text);
    cursor.cursor_pos = paste_position + text.chars().count();
    sel.apply_primary_cursor(&cursor);
    tv.content_version += 1;

    hist.history.record(EditOperation {
        removed_text: deleted_text,
        inserted_text: text.clone(),
        position: paste_position,
        cursor_before,
        cursor_after: cursor.cursor_pos,
        kind: EditKind::Paste,
    });
}
