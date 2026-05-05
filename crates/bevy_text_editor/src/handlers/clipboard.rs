//! Clipboard handlers — Copy / Cut / Paste.

use crate::editing_events::*;
use crate::history::EditKind;
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
    let selected_text = tv.rope.slice(
        start.min(tv.rope.len_chars())..end.min(tv.rope.len_chars()),
    ).to_string();

    if let Ok(mut clipboard) = Clipboard::new() {
        let _ = clipboard.set_text(selected_text);
    }

    let outcome = hist.replace_range(&mut tv, start, end, "", EditKind::Other, true);
    cursor.cursor_pos = outcome.new_cursor_pos;
    sel.apply_primary_cursor(&cursor);
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

    let (start, end) = sel.primary_range().unwrap_or((cursor.cursor_pos, cursor.cursor_pos));
    let outcome = hist.replace_range(&mut tv, start, end, &text, EditKind::Paste, true);
    cursor.cursor_pos = outcome.new_cursor_pos;
    sel.apply_primary_cursor(&cursor);
}
