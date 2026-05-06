//! Code-folding handlers — ToggleFold, Fold, Unfold, FoldAll, UnfoldAll.

use crate::input::action_events::*;
use crate::types::*;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;

pub fn handle_toggle_fold(
    mut events: MessageReader<ToggleFoldRequested>,
    input_focus: Res<InputFocus>,
    mut q: Query<
        (
            &CursorState,
            &crate::text_view::TextBuffer,
            &mut FoldState,
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
    let Ok((cursor, buffer, mut fold_state)) = q.get_mut(entity) else {
        return;
    };
    let line = buffer.rope.char_to_line(cursor.cursor_pos);
    fold_state.toggle_fold_at_line(line);
}

pub fn handle_fold(
    mut events: MessageReader<FoldRequested>,
    input_focus: Res<InputFocus>,
    mut q: Query<
        (
            &CursorState,
            &crate::text_view::TextBuffer,
            &mut FoldState,
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
    let Ok((cursor, buffer, mut fold_state)) = q.get_mut(entity) else {
        return;
    };
    let line = buffer.rope.char_to_line(cursor.cursor_pos);
    fold_state.fold_at_line(line);
}

pub fn handle_unfold(
    mut events: MessageReader<UnfoldRequested>,
    input_focus: Res<InputFocus>,
    mut q: Query<
        (
            &CursorState,
            &crate::text_view::TextBuffer,
            &mut FoldState,
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
    let Ok((cursor, buffer, mut fold_state)) = q.get_mut(entity) else {
        return;
    };
    let line = buffer.rope.char_to_line(cursor.cursor_pos);
    fold_state.unfold_at_line(line);
}

pub fn handle_fold_all(
    mut events: MessageReader<FoldAllRequested>,
    input_focus: Res<InputFocus>,
    mut q: Query<&mut FoldState, With<CodeEditor>>,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok(mut fold_state) = q.get_mut(entity) else {
        return;
    };
    fold_state.fold_all();
}

pub fn handle_unfold_all(
    mut events: MessageReader<UnfoldAllRequested>,
    input_focus: Res<InputFocus>,
    mut q: Query<&mut FoldState, With<CodeEditor>>,
) {
    if events.read().next().is_none() {
        return;
    }
    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok(mut fold_state) = q.get_mut(entity) else {
        return;
    };
    fold_state.unfold_all();
}
