//! Code-folding handlers — ToggleFold, Fold, Unfold, FoldAll, UnfoldAll,
//! plus the change-detection system that fans `is_folded` transitions
//! onto the message bus as `EditorFoldStateChanged`.

use std::collections::HashMap;

use crate::input::action_events::*;
use crate::types::events::EditorFoldStateChanged;
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

/// Watches `Changed<FoldState>` and emits one `EditorFoldStateChanged` per
/// region whose `is_folded` flipped since the last frame. The fold-region
/// detector bumps `content_version` (re-parse) without changing fold flags;
/// without per-region diffing hosts would see a flood of false positives on
/// every keystroke.
pub fn emit_fold_state_changed(
    q: Query<(Entity, &FoldState), (With<CodeEditor>, Changed<FoldState>)>,
    mut writer: MessageWriter<EditorFoldStateChanged>,
    mut last_known: Local<HashMap<(Entity, usize), bool>>,
) {
    let mut seen: HashMap<(Entity, usize), bool> =
        HashMap::with_capacity(last_known.len());
    for (entity, state) in q.iter() {
        for region in &state.regions {
            let key = (entity, region.start_line);
            seen.insert(key, region.is_folded);
            let prev = last_known.get(&key).copied();
            if prev != Some(region.is_folded) {
                writer.write(EditorFoldStateChanged {
                    entity,
                    start_line: region.start_line,
                    is_folded: region.is_folded,
                });
            }
        }
    }
    // Drop entries for regions that no longer exist (re-parse merged/split
    // them or the editor was despawned). Re-introducing the same start_line
    // counts as a fresh transition, which is the right behavior.
    last_known.retain(|key, _| seen.contains_key(key));
    for (key, val) in seen {
        last_known.insert(key, val);
    }
}
