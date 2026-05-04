//! Editor keyboard handling.
//!
//! Split into two pieces:
//!
//! 1. [`on_focused_keyboard`] — an observer on
//!    [`bevy::input_focus::FocusedInput<KeyboardInput>`]. Per-event work:
//!    rename-modal routing, character insertion, bracket / quote auto-close,
//!    LSP completion triggers. Picking + focus dispatch already routed the
//!    event to the focused editor entity, so there's no manual focus check.
//!
//! 2. [`process_editor_actions`] — a `Res<InputFocus>`-filtered polling
//!    system that consumes `ActionState<EditorAction>` (leafwing). Handles
//!    just-pressed actions, key repeat, and dispatches to `execute_action`.
//!    Stays a polling system because `ActionState` is itself polled state,
//!    not an event stream.

use super::actions::{
    execute_action, get_closing_bracket, get_closing_quote, insert_char, insert_closing_char,
    should_skip_auto_close,
};
#[cfg(feature = "lsp")]
use super::actions::{
    find_word_start, request_completion, send_did_change, update_completion_filter,
};
use super::editor_ops::move_cursor;
use super::keybindings::EditorAction;
use crate::plugin::EditorInputManager;
#[cfg(feature = "lsp")]
use crate::settings::LspSettings;
use crate::settings::{BracketSettings, CursorSettings, IndentationSettings};
use crate::text_view::{TextViewState, TextViewViewport};
use crate::types::*;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput};
use bevy::input_focus::{FocusedInput, InputFocus};
use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use std::time::Instant;

const ALL_ACTIONS: [EditorAction; 45] = [
    EditorAction::DeleteBackward,
    EditorAction::DeleteForward,
    EditorAction::DeleteWordBackward,
    EditorAction::DeleteWordForward,
    EditorAction::DeleteLine,
    EditorAction::InsertNewline,
    EditorAction::InsertTab,
    EditorAction::MoveCursorLeft,
    EditorAction::MoveCursorRight,
    EditorAction::MoveCursorUp,
    EditorAction::MoveCursorDown,
    EditorAction::MoveCursorWordLeft,
    EditorAction::MoveCursorWordRight,
    EditorAction::MoveCursorLineStart,
    EditorAction::MoveCursorLineEnd,
    EditorAction::MoveCursorDocumentStart,
    EditorAction::MoveCursorDocumentEnd,
    EditorAction::MoveCursorPageUp,
    EditorAction::MoveCursorPageDown,
    EditorAction::SelectLeft,
    EditorAction::SelectRight,
    EditorAction::SelectUp,
    EditorAction::SelectDown,
    EditorAction::SelectWordLeft,
    EditorAction::SelectWordRight,
    EditorAction::SelectLineStart,
    EditorAction::SelectLineEnd,
    EditorAction::SelectAll,
    EditorAction::ClearSelection,
    EditorAction::Copy,
    EditorAction::Cut,
    EditorAction::Paste,
    EditorAction::Undo,
    EditorAction::Redo,
    EditorAction::Replace,
    EditorAction::GotoLine,
    EditorAction::RequestCompletion,
    EditorAction::GotoDefinition,
    EditorAction::RenameSymbol,
    EditorAction::AddCursorAtNextOccurrence,
    EditorAction::AddCursorAbove,
    EditorAction::AddCursorBelow,
    EditorAction::ClearSecondaryCursors,
    EditorAction::Save,
    EditorAction::Open,
];

/// True when any modifier key is held — used by the char observer to skip
/// shortcut keystrokes (Ctrl+C, Cmd+S, etc.) that should be handled by
/// `process_editor_actions` via leafwing's `ActionState`, not inserted as
/// raw characters.
fn modifier_held(keyboard: &ButtonInput<KeyCode>) -> bool {
    keyboard.pressed(KeyCode::ControlLeft)
        || keyboard.pressed(KeyCode::ControlRight)
        || keyboard.pressed(KeyCode::SuperLeft)
        || keyboard.pressed(KeyCode::SuperRight)
        || keyboard.pressed(KeyCode::AltLeft)
        || keyboard.pressed(KeyCode::AltRight)
}

/// Per-event observer for keyboard input dispatched to the focused editor.
///
/// `bevy_input_focus` already routed this event because the editor entity
/// is in `InputFocus`. We never manually compare to `input_focus.get()`.
#[allow(clippy::too_many_arguments)]
pub fn on_focused_keyboard(
    trigger: On<FocusedInput<KeyboardInput>>,
    mut editor_query: Query<
        (
            &mut SelectionState,
            &mut EditHistoryState,
            &mut SyntaxCacheState,
            &mut EditorDisplayState,
            &mut CursorState,
            &mut TextViewState,
        ),
        With<CodeEditor>,
    >,
    keyboard: Res<ButtonInput<KeyCode>>,
    brackets: Res<BracketSettings>,
    #[cfg(feature = "lsp")] lsp: Res<LspSettings>,
    #[cfg(feature = "lsp")] lsp_client: Res<crate::lsp::LspClient>,
    #[cfg(feature = "lsp")] mut completion_state: ResMut<crate::lsp::CompletionState>,
    #[cfg(feature = "lsp")] mut rename_state: ResMut<crate::lsp::RenameState>,
    #[cfg(feature = "lsp")] mut lsp_sync: ResMut<crate::lsp::LspSyncState>,
) {
    let entity = trigger.event().focused_entity;
    let Ok((mut sel, mut hist, mut syntax, mut display, mut cursor, mut tv)) =
        editor_query.get_mut(entity)
    else {
        return;
    };

    let event = &trigger.event().input;
    if !event.state.is_pressed() {
        return;
    }

    // Rename modal eats input until dismissed.
    #[cfg(feature = "lsp")]
    if rename_state.visible {
        match &event.logical_key {
            Key::Character(text) => {
                for c in text.chars() {
                    if !c.is_control() {
                        rename_state.new_name.push(c);
                    }
                }
            }
            Key::Space => rename_state.new_name.push(' '),
            Key::Backspace => {
                rename_state.new_name.pop();
            }
            Key::Enter => {
                if rename_state.can_submit() {
                    if let (Some(position), Some(uri)) =
                        (rename_state.position, lsp_sync.document_uri.as_ref())
                    {
                        crate::lsp::systems::execute_rename(
                            &lsp_client,
                            uri,
                            position,
                            rename_state.new_name.clone(),
                        );
                    }
                }
                rename_state.reset();
            }
            Key::Escape => rename_state.reset(),
            _ => {}
        }
        return;
    }

    // Shortcut keystrokes (Ctrl+C, Cmd+S, …) are handled by the action
    // system; the char observer must not insert their key as text.
    if modifier_held(&keyboard) {
        return;
    }

    match &event.logical_key {
        Key::Character(text) => {
            for c in text.chars() {
                if c.is_control() {
                    continue;
                }
                insert_typed_char(
                    c,
                    &mut sel,
                    &mut hist,
                    &mut syntax,
                    &mut display,
                    &mut cursor,
                    &mut tv,
                    &brackets,
                    #[cfg(feature = "lsp")]
                    &lsp,
                    #[cfg(feature = "lsp")]
                    &lsp_client,
                    #[cfg(feature = "lsp")]
                    &mut completion_state,
                    #[cfg(feature = "lsp")]
                    &mut lsp_sync,
                );
            }
        }
        Key::Space => {
            insert_char(
                &mut sel,
                &mut hist,
                &mut syntax,
                &mut display,
                &mut cursor,
                &mut tv,
                ' ',
            );
            #[cfg(feature = "lsp")]
            {
                send_did_change(&tv.rope, &lsp_client, &mut lsp_sync);
                completion_state.visible = false;
            }
        }
        _ => {}
    }
}

/// Insert one typed character with bracket/quote auto-close + LSP triggers.
///
/// Pulled out of the main observer to keep the per-event match arm small.
#[allow(clippy::too_many_arguments)]
fn insert_typed_char(
    c: char,
    sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    syntax: &mut SyntaxCacheState,
    display: &mut EditorDisplayState,
    cursor: &mut CursorState,
    tv: &mut TextViewState,
    brackets: &BracketSettings,
    #[cfg(feature = "lsp")] lsp: &LspSettings,
    #[cfg(feature = "lsp")] lsp_client: &crate::lsp::LspClient,
    #[cfg(feature = "lsp")] completion_state: &mut crate::lsp::CompletionState,
    #[cfg(feature = "lsp")] lsp_sync: &mut crate::lsp::LspSyncState,
) {
    // Skip over an existing matching close char rather than inserting a duplicate.
    if brackets.auto_close_quotes
        && get_closing_quote(c).is_some()
        && should_skip_auto_close(cursor, &tv.rope, c)
    {
        move_cursor(cursor, &tv.rope, 1);
        return;
    }
    if brackets.auto_close {
        let is_closing_bracket = brackets.pairs.iter().any(|(_, close)| *close == c);
        if is_closing_bracket && should_skip_auto_close(cursor, &tv.rope, c) {
            move_cursor(cursor, &tv.rope, 1);
            return;
        }
    }

    insert_char(sel, hist, syntax, display, cursor, tv, c);

    if brackets.auto_close {
        if let Some(closing) = get_closing_bracket(c, &brackets.pairs) {
            insert_closing_char(cursor, tv, closing);
        }
    }
    if brackets.auto_close_quotes {
        if let Some(closing) = get_closing_quote(c) {
            // Don't auto-close ' mid-word ("don't").
            let should_close = if c == '\'' {
                let cur_pos = cursor.cursor_pos;
                if cur_pos >= 2 {
                    !tv.rope.char(cur_pos - 2).is_alphanumeric()
                } else {
                    true
                }
            } else {
                true
            };
            if should_close {
                insert_closing_char(cursor, tv, closing);
            }
        }
    }

    #[cfg(feature = "lsp")]
    {
        send_did_change(&tv.rope, lsp_client, lsp_sync);

        if lsp.completion.enabled {
            let mut is_trigger = false;
            let cursor_pos = cursor.cursor_pos;

            for trigger in &lsp.completion.trigger_characters {
                if trigger.len() == 1 {
                    if c.to_string() == *trigger {
                        is_trigger = true;
                        break;
                    }
                } else if cursor_pos >= trigger.len() {
                    let start = cursor_pos - trigger.len();
                    let recent_text: String =
                        tv.rope.slice(start..cursor_pos).chars().collect();
                    if recent_text == *trigger {
                        is_trigger = true;
                        break;
                    }
                }
            }

            if is_trigger {
                completion_state.visible = false;
                request_completion(cursor, &tv.rope, lsp_client, completion_state, lsp_sync);
            } else if c.is_alphanumeric() || c == '_' {
                if completion_state.visible {
                    update_completion_filter(cursor, &tv.rope, completion_state);
                } else {
                    let word_start = find_word_start(&tv.rope, cursor.cursor_pos);
                    let word_len = cursor.cursor_pos - word_start;
                    if word_len >= lsp.completion.min_word_length {
                        completion_state.start_char_index = word_start;
                        request_completion(
                            cursor,
                            &tv.rope,
                            lsp_client,
                            completion_state,
                            lsp_sync,
                        );
                    }
                }
            } else if completion_state.visible {
                completion_state.visible = false;
                completion_state.filter.clear();
            }
        }
    }
}

/// Polling system that drives leafwing actions for the focused editor.
///
/// Reads `Res<InputFocus>` to pick the editor whose actions should fire,
/// then runs the standard just-pressed / key-repeat dispatch. Replaces the
/// pre-Phase-7 iter+skip pattern with a single direct lookup.
#[allow(clippy::too_many_arguments)]
pub fn process_editor_actions(
    mut editor_query: Query<
        (
            &mut SelectionState,
            &mut EditHistoryState,
            &mut SyntaxCacheState,
            &mut EditorDisplayState,
            &mut CursorState,
            &mut TextViewState,
            &TextViewViewport,
            &mut GotoLineState,
            &mut FoldState,
        ),
        With<CodeEditor>,
    >,
    input_focus: Res<InputFocus>,
    action_query: Query<&ActionState<EditorAction>, With<EditorInputManager>>,
    cursor_settings: Res<CursorSettings>,
    indentation: Res<IndentationSettings>,
    #[cfg(feature = "lsp")] lsp: Res<LspSettings>,
    mut key_repeat_state: ResMut<KeyRepeatState>,
    mut save_events: MessageWriter<crate::types::SaveRequested>,
    mut open_events: MessageWriter<crate::types::OpenRequested>,
    #[cfg(feature = "lsp")] lsp_client: Res<crate::lsp::LspClient>,
    #[cfg(feature = "lsp")] mut completion_state: ResMut<crate::lsp::CompletionState>,
    #[cfg(feature = "lsp")] mut rename_state: ResMut<crate::lsp::RenameState>,
    #[cfg(feature = "lsp")] mut lsp_sync: ResMut<crate::lsp::LspSyncState>,
) {
    let Some(focused) = input_focus.get() else {
        return;
    };
    let Ok(action_state) = action_query.single() else {
        warn!("No EditorInputManager entity found with ActionState");
        return;
    };

    #[cfg(feature = "lsp")]
    if rename_state.visible {
        // Rename modal owns input; the observer routes char events into the
        // modal's buffer. Skip action processing entirely.
        return;
    }

    let Ok((
        mut sel,
        mut hist,
        mut syntax,
        mut display,
        mut cursor,
        mut tv,
        _viewport,
        mut goto_line_state,
        mut fold_state,
    )) = editor_query.get_mut(focused)
    else {
        return;
    };

    let mut action_to_execute: Option<EditorAction> = None;
    let now = Instant::now();

    for action in ALL_ACTIONS {
        if action_state.just_pressed(&action) {
            action_to_execute = Some(action);

            if action.is_repeatable() {
                key_repeat_state.current_action = Some(action);
                key_repeat_state.press_start = Some(now);
                key_repeat_state.last_repeat = None;
            }
            break;
        }
    }

    // Folding actions are checked separately to keep ALL_ACTIONS small.
    if action_to_execute.is_none() {
        for action in [
            EditorAction::ToggleFold,
            EditorAction::Fold,
            EditorAction::Unfold,
            EditorAction::FoldAll,
            EditorAction::UnfoldAll,
        ] {
            if action_state.just_pressed(&action) {
                action_to_execute = Some(action);
                break;
            }
        }
    }

    // Key repeat on held actions.
    if action_to_execute.is_none() {
        if let Some(current_action) = key_repeat_state.current_action {
            if action_state.pressed(&current_action) {
                let initial_delay = cursor_settings.key_repeat.initial_delay_ms as f64 / 1000.0;
                let repeat_interval = cursor_settings.key_repeat.repeat_delay_ms as f64 / 1000.0;

                if let Some(press_start) = key_repeat_state.press_start {
                    let elapsed = now.duration_since(press_start).as_secs_f64();
                    if elapsed >= initial_delay {
                        let should_repeat = match key_repeat_state.last_repeat {
                            Some(last) => {
                                now.duration_since(last).as_secs_f64() >= repeat_interval
                            }
                            None => true,
                        };
                        if should_repeat {
                            action_to_execute = Some(current_action);
                            key_repeat_state.last_repeat = Some(now);
                        }
                    }
                }
            } else {
                key_repeat_state.current_action = None;
                key_repeat_state.press_start = None;
                key_repeat_state.last_repeat = None;
            }
        }
    }

    let Some(action) = action_to_execute else {
        return;
    };

    if action == EditorAction::Save {
        let content: String = tv.rope.chars().collect();
        save_events.write(crate::types::SaveRequested { content });
        return;
    }

    if action == EditorAction::Open {
        open_events.write(crate::types::OpenRequested);
        return;
    }

    #[cfg(feature = "lsp")]
    if action == EditorAction::RenameSymbol {
        if lsp_client.capabilities.supports_rename() {
            if let Some(uri) = &lsp_sync.document_uri {
                let cp = cursor.cursor_pos.min(tv.rope.len_chars());
                let line = tv.rope.char_to_line(cp);
                let line_start = tv.rope.line_to_char(line);
                let character = cp - line_start;
                let position = lsp_types::Position {
                    line: line as u32,
                    character: character as u32,
                };
                rename_state.start_prepare(position);
                crate::lsp::systems::request_prepare_rename(&lsp_client, uri, position);
            }
        }
        return;
    }

    execute_action(
        EditorBuf {
            sel: &mut sel,
            hist: &mut hist,
            syntax: &mut syntax,
            display: &mut display,
            cursor: &mut cursor,
            tv: &mut tv,
        },
        action,
        &indentation,
        &mut goto_line_state,
        &mut fold_state,
        #[cfg(feature = "lsp")]
        crate::input::actions::LspBuf {
            settings: &lsp,
            client: &lsp_client,
            completion: &mut completion_state,
            sync: &mut lsp_sync,
        },
    );
}
