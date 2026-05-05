//! Per-event keyboard observer for the focused editor.
//!
//! Only one observer lives here: [`on_focused_keyboard`], which runs on
//! [`bevy::input_focus::FocusedInput<KeyboardInput>`]. It handles
//! rename-modal routing, character insertion, bracket / quote auto-close,
//! and LSP completion triggers.
//!
//! Action-based input (just-pressed / repeating shortcuts via leafwing's
//! `ActionState`) was previously also handled here in `process_editor_actions`.
//! That function is gone — its responsibilities are split between
//! [`super::dispatch::dispatch_action_events`] (event emission) and the
//! per-action handler systems under [`super::handlers`].

use super::actions::{
    get_closing_bracket, get_closing_quote, insert_closing_char, should_skip_auto_close,
};
#[cfg(feature = "lsp")]
use super::actions::{
    find_word_start, request_completion, send_did_change, update_completion_filter,
};
use super::editing::drain_one;
use super::editor_ops::move_cursor;
#[cfg(feature = "lsp")]
use crate::settings::LspSettings;
use crate::settings::BracketSettings;
use crate::types::*;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput};
use bevy::input_focus::FocusedInput;
use bevy::prelude::*;

/// True when any modifier key is held — used by the char observer to skip
/// shortcut keystrokes (Ctrl+C, Cmd+S, etc.) that should be handled by
/// the action dispatcher via leafwing's `ActionState`, not inserted as
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
            &mut crate::text_view::TextViewState,
        ),
        With<CodeEditor>,
    >,
    #[cfg(feature = "lsp")] mut lsp_query: Query<
        (
            &bevy_lsp::LspClient,
            Option<&mut bevy_lsp::LspDocument>,
            &bevy_lsp::ServerCapabilities,
            &mut crate::lsp_ui::state::LspCompletionPopup,
            &mut crate::lsp_ui::state::LspRenamePopup,
        ),
        With<CodeEditor>,
    >,
    keyboard: Res<ButtonInput<KeyCode>>,
    brackets: Res<BracketSettings>,
    #[cfg(feature = "lsp")] lsp: Res<LspSettings>,
) {
    let entity = trigger.event().focused_entity;

    let Ok((mut sel, mut hist, mut syntax, mut display, mut cursor, mut tv)) =
        editor_query.get_mut(entity)
    else {
        return;
    };

    #[cfg(feature = "lsp")]
    let Ok((
        lsp_client,
        mut lsp_document,
        _capabilities,
        mut completion_state,
        mut rename_state,
    )) = lsp_query.get_mut(entity)
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
                    if let (Some(position), Some(doc)) =
                        (rename_state.position, lsp_document.as_deref())
                    {
                        crate::lsp_ui::systems::execute_rename(
                            lsp_client,
                            _capabilities,
                            &doc.uri,
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
    // dispatcher; the char observer must not insert their key as text.
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
                    lsp_client,
                    #[cfg(feature = "lsp")]
                    &mut completion_state,
                    #[cfg(feature = "lsp")]
                    lsp_document.as_deref_mut(),
                );
            }
        }
        Key::Space => {
            bevy_text_editor::handlers::edit::insert_char(
                &mut sel, &mut hist, &mut cursor, &mut tv, ' ',
            );
            drain_one(&mut hist, &mut syntax, &mut display);
            #[cfg(feature = "lsp")]
            {
                send_did_change(&tv.rope, lsp_client, lsp_document.as_deref_mut());
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
    tv: &mut crate::text_view::TextViewState,
    brackets: &BracketSettings,
    #[cfg(feature = "lsp")] lsp: &LspSettings,
    #[cfg(feature = "lsp")] lsp_client: &bevy_lsp::LspClient,
    #[cfg(feature = "lsp")] completion_state: &mut crate::lsp_ui::state::LspCompletionPopup,
    #[cfg(feature = "lsp")] mut lsp_document: Option<&mut bevy_lsp::LspDocument>,
) {
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

    bevy_text_editor::handlers::edit::insert_char(sel, hist, cursor, tv, c);
    drain_one(hist, syntax, display);

    if brackets.auto_close {
        if let Some(closing) = get_closing_bracket(c, &brackets.pairs) {
            insert_closing_char(cursor, tv, closing);
        }
    }
    if brackets.auto_close_quotes {
        if let Some(closing) = get_closing_quote(c) {
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
        send_did_change(&tv.rope, lsp_client, lsp_document.as_deref_mut());

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
                    let recent_text: String = tv.rope.slice(start..cursor_pos).chars().collect();
                    if recent_text == *trigger {
                        is_trigger = true;
                        break;
                    }
                }
            }

            if is_trigger {
                completion_state.visible = false;
                request_completion(
                    cursor,
                    &tv.rope,
                    lsp_client,
                    completion_state,
                    lsp_document.as_deref(),
                );
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
                            lsp_document.as_deref(),
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
