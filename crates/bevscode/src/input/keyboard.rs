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

#[cfg(feature = "lsp")]
use super::actions::{find_word_start, request_completion, update_completion_filter};
use bevy_text_editor::RopeBuffer;
use super::actions::{
    get_closing_bracket, get_closing_quote, insert_closing_char, should_skip_auto_close,
};
use super::editor_ops::move_cursor;
use crate::settings::BracketConfig;
#[cfg(feature = "lsp")]
use crate::settings::LspConfig;
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

#[cfg(feature = "lsp")]
type KeyboardLspQuery<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static mut bevy_lsp::LspDocument>,
        &'static bevy_lsp::ServerCapabilities,
        &'static mut crate::lsp_ui::state::LspCompletionPopup,
        &'static mut crate::lsp_ui::state::LspRenamePopup,
        Option<&'static bevy_tree_sitter::SyntaxTree>,
        &'static LspConfig,
    ),
    With<CodeEditor>,
>;

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
            &mut CursorState,
            &mut crate::text_view::TextBuffer<RopeBuffer>,
            &BracketConfig,
        ),
        With<CodeEditor>,
    >,
    #[cfg(feature = "lsp")] mut lsp_query: KeyboardLspQuery,
    #[cfg(feature = "lsp")] mut lsp_w: MessageWriter<bevy_lsp::LspRequest>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    let entity = trigger.event().focused_entity;

    let Ok((mut sel, mut hist, mut cursor, mut buffer, brackets)) = editor_query.get_mut(entity)
    else {
        return;
    };

    #[cfg(feature = "lsp")]
    let Ok((
        mut lsp_document,
        capabilities,
        mut completion_state,
        mut rename_state,
        syntax_tree,
        lsp,
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
                            entity,
                            capabilities,
                            &doc.uri,
                            position,
                            rename_state.new_name.clone(),
                            &mut lsp_w,
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
                    &mut cursor,
                    &mut buffer,
                    brackets,
                    #[cfg(feature = "lsp")]
                    lsp,
                    #[cfg(feature = "lsp")]
                    entity,
                    #[cfg(feature = "lsp")]
                    capabilities,
                    #[cfg(feature = "lsp")]
                    &mut completion_state,
                    #[cfg(feature = "lsp")]
                    lsp_document.as_deref_mut(),
                    #[cfg(feature = "lsp")]
                    syntax_tree,
                    #[cfg(feature = "lsp")]
                    &mut lsp_w,
                );
            }
        }
        Key::Space => {
            bevy_text_editor::handlers::edit::insert_char(
                &mut sel,
                &mut hist,
                &mut cursor,
                &mut buffer,
                ' ',
            );
            #[cfg(feature = "lsp")]
            {
                let _ = &lsp_document;
                completion_state.dismiss();
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
    cursor: &mut CursorState,
    buffer: &mut crate::text_view::TextBuffer<RopeBuffer>,
    brackets: &BracketConfig,
    #[cfg(feature = "lsp")] lsp: &LspConfig,
    #[cfg(feature = "lsp")] entity: Entity,
    #[cfg(feature = "lsp")] capabilities: &bevy_lsp::ServerCapabilities,
    #[cfg(feature = "lsp")] completion_state: &mut crate::lsp_ui::state::LspCompletionPopup,
    #[cfg(feature = "lsp")] lsp_document: Option<&mut bevy_lsp::LspDocument>,
    #[cfg(feature = "lsp")] syntax_tree: Option<&bevy_tree_sitter::SyntaxTree>,
    #[cfg(feature = "lsp")] mut lsp_w: &mut MessageWriter<bevy_lsp::LspRequest>,
) {
    if brackets.auto_close_quotes
        && get_closing_quote(c).is_some()
        && should_skip_auto_close(cursor, buffer.rope(), c)
    {
        move_cursor(cursor, buffer.rope(), 1);
        return;
    }
    if brackets.auto_close {
        let is_closing_bracket = brackets.pairs.iter().any(|(_, close)| *close == c);
        if is_closing_bracket && should_skip_auto_close(cursor, buffer.rope(), c) {
            move_cursor(cursor, buffer.rope(), 1);
            return;
        }
    }

    bevy_text_editor::handlers::edit::insert_char(sel, hist, cursor, buffer, c);

    if brackets.auto_close {
        if let Some(closing) = get_closing_bracket(c, &brackets.pairs) {
            insert_closing_char(cursor, buffer, closing);
        }
    }
    if brackets.auto_close_quotes {
        if let Some(closing) = get_closing_quote(c) {
            let should_close = if c == '\'' {
                let cur_pos = cursor.cursor_pos;
                if cur_pos >= 2 {
                    !buffer.char(cur_pos - 2).is_alphanumeric()
                } else {
                    true
                }
            } else {
                true
            };
            if should_close {
                insert_closing_char(cursor, buffer, closing);
            }
        }
    }

    #[cfg(feature = "lsp")]
    {
        // didChange is emitted via `listen_text_edit_events` from the
        // OnEdit pipeline — no need to send it here.

        if lsp.completion.enabled {
            let cursor_pos = cursor.cursor_pos;

            // Suppress completion requests while the cursor is inside a
            // string or comment per tree-sitter — Zed's "is_completion_context"
            // gate. When tree-sitter isn't ready, default to allow.
            let in_completion_context = match syntax_tree.and_then(|st| st.tree.as_ref()) {
                #[cfg(feature = "tree-sitter")]
                Some(tree) => {
                    let byte = buffer.char_to_byte(cursor_pos);
                    crate::plugin::syntax_highlighting::EditorSyntaxState::is_completion_context(
                        tree, byte,
                    )
                }
                _ => true,
            };

            // Prefer the LSP server's advertised triggers; fall back to the
            // host's configured list when the server doesn't advertise any.
            let server_triggers = capabilities.completion_triggers();
            let triggers: &[String] = if !server_triggers.is_empty() {
                &server_triggers
            } else {
                &lsp.completion.trigger_characters
            };

            let mut is_trigger = false;
            for trigger in triggers {
                if trigger.len() == 1 {
                    if c.to_string() == *trigger {
                        is_trigger = true;
                        break;
                    }
                } else if cursor_pos >= trigger.len() {
                    let start = cursor_pos - trigger.len();
                    let recent_text: String =
                        buffer.slice(start..cursor_pos).chars().collect();
                    if recent_text == *trigger {
                        is_trigger = true;
                        break;
                    }
                }
            }

            if is_trigger && in_completion_context {
                completion_state.dismiss();
                request_completion(
                    entity,
                    cursor,
                    buffer.rope(),
                    completion_state,
                    lsp_document.as_deref(),
                    &mut lsp_w,
                );
            } else if (c.is_alphanumeric() || c == '_') && in_completion_context {
                if completion_state.visible {
                    update_completion_filter(cursor, buffer.rope(), completion_state);
                } else {
                    let word_start = find_word_start(buffer.rope(), cursor.cursor_pos);
                    let word_len = cursor.cursor_pos - word_start;
                    if word_len >= lsp.completion.min_word_length {
                        completion_state.start_char_index = word_start;
                        request_completion(
                            entity,
                            cursor,
                            buffer.rope(),
                            completion_state,
                            lsp_document.as_deref(),
                            &mut lsp_w,
                        );
                    }
                }
            } else if completion_state.visible {
                completion_state.dismiss();
            }
        }
    }
}
