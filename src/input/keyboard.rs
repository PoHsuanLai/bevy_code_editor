use super::actions::{
    execute_action, get_closing_bracket, get_closing_quote, insert_char, insert_closing_char,
    should_skip_auto_close,
};
#[cfg(feature = "lsp")]
use super::actions::{
    find_word_start, request_completion, send_did_change, update_completion_filter,
};
use super::keybindings::EditorAction;
use crate::plugin::EditorInputManager;
#[cfg(feature = "lsp")]
use crate::settings::LspSettings;
use crate::settings::{BracketSettings, CursorSettings, IndentationSettings};
use crate::text_view::{TextViewState, TextViewViewport};
use crate::types::*;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use std::time::Instant;

/// All possible editor actions for iteration
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

/// System to handle keyboard input using leafwing-input-manager
pub fn handle_keyboard_input(
    mut editor_query: Query<(&mut CodeEditorState, &mut TextViewState, &TextViewViewport), With<CodeEditor>>,
    mut char_events: MessageReader<KeyboardInput>,
    action_query: Query<&ActionState<EditorAction>, With<EditorInputManager>>,
    cursor: Res<CursorSettings>,
    brackets: Res<BracketSettings>,
    indentation: Res<IndentationSettings>,
    #[cfg(feature = "lsp")] lsp: Res<LspSettings>,
    mut goto_line_state: ResMut<GotoLineState>,
    #[cfg(feature = "folding")] mut fold_state: ResMut<FoldState>,
    mut key_repeat_state: ResMut<KeyRepeatState>,
    (mut save_events, mut open_events): (
        MessageWriter<crate::types::SaveRequested>,
        MessageWriter<crate::types::OpenRequested>,
    ),
    #[cfg(feature = "lsp")] lsp_client: Res<crate::lsp::LspClient>,
    #[cfg(feature = "lsp")] mut completion_state: ResMut<crate::lsp::CompletionState>,
    #[cfg(feature = "lsp")] mut rename_state: ResMut<crate::lsp::state::RenameState>,
    #[cfg(feature = "lsp")] mut lsp_sync: ResMut<crate::lsp::LspSyncState>,
) {
    let Ok((mut state, mut tv, _viewport)) = editor_query.single_mut() else { return; };

    // Only process input if editor is focused
    if !state.is_focused {
        return;
    }

    let Ok(action_state) = action_query.single() else {
        warn!("No EditorInputManager entity found with ActionState");
        return;
    };

    // Handle rename dialog input (LSP feature)
    #[cfg(feature = "lsp")]
    if rename_state.visible {
        // Rename dialog is active - capture input for the rename text field
        for event in char_events.read() {
            if !event.state.is_pressed() {
                continue;
            }

            match &event.logical_key {
                bevy::input::keyboard::Key::Character(ref text) => {
                    for c in text.chars() {
                        if !c.is_control() {
                            rename_state.new_name.push(c);
                        }
                    }
                }
                bevy::input::keyboard::Key::Space => {
                    rename_state.new_name.push(' ');
                }
                bevy::input::keyboard::Key::Backspace => {
                    rename_state.new_name.pop();
                }
                bevy::input::keyboard::Key::Enter => {
                    // Submit rename
                    if rename_state.can_submit() {
                        if let Some(position) = rename_state.position {
                            if let Some(uri) = &lsp_sync.document_uri {
                                crate::lsp::systems::execute_rename(
                                    &lsp_client,
                                    uri,
                                    position,
                                    rename_state.new_name.clone(),
                                );
                            }
                        }
                    }
                    rename_state.reset();
                }
                bevy::input::keyboard::Key::Escape => {
                    // Cancel rename
                    rename_state.reset();
                }
                _ => {}
            }
        }
        // Consume all events and return - don't process normal editor input
        return;
    }

    let mut action_to_execute: Option<EditorAction> = None;
    let now = Instant::now();

    // First, check for any newly pressed action
    for action in ALL_ACTIONS {
        if action_state.just_pressed(&action) {
            action_to_execute = Some(action);

            // If this is a repeatable action, start tracking it
            if action.is_repeatable() {
                key_repeat_state.current_action = Some(action);
                key_repeat_state.press_start = Some(now);
                key_repeat_state.last_repeat = None;
            }
            break;
        }
    }

    // Also check code folding actions (not in ALL_ACTIONS to keep array size reasonable)
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

    // If no new press, check for key repeat on held actions
    if action_to_execute.is_none() {
        if let Some(current_action) = key_repeat_state.current_action {
            // Check if the action is still being held
            if action_state.pressed(&current_action) {
                let initial_delay = cursor.key_repeat.initial_delay_ms as f64 / 1000.0;
                let repeat_interval = cursor.key_repeat.repeat_delay_ms as f64 / 1000.0;

                if let Some(press_start) = key_repeat_state.press_start {
                    let elapsed = now.duration_since(press_start).as_secs_f64();

                    // Check if we've passed the initial delay
                    if elapsed >= initial_delay {
                        // Check if it's time for a repeat
                        let should_repeat = match key_repeat_state.last_repeat {
                            Some(last) => now.duration_since(last).as_secs_f64() >= repeat_interval,
                            None => true, // First repeat after initial delay
                        };

                        if should_repeat {
                            action_to_execute = Some(current_action);
                            key_repeat_state.last_repeat = Some(now);
                        }
                    }
                }
            } else {
                // Key was released, clear the repeat state
                key_repeat_state.current_action = None;
                key_repeat_state.press_start = None;
                key_repeat_state.last_repeat = None;
            }
        }
    }

    // Handle character input (for printable characters)
    // Only process if no keybinding action was triggered
    if action_to_execute.is_none() {
        for event in char_events.read() {
            // Only handle key presses with text
            if event.state.is_pressed() {
                match &event.logical_key {
                    bevy::input::keyboard::Key::Character(ref text) => {
                        for c in text.chars() {
                            // Skip control characters (they're handled by keybindings)
                            if c.is_control() {
                                continue;
                            }

                            // Check for quote skip-over (typing closing quote when already there)
                            if brackets.auto_close_quotes
                                && get_closing_quote(c).is_some()
                                && should_skip_auto_close(&state, c)
                            {
                                // Just move cursor past the existing quote
                                state.move_cursor(1);
                                tv.pending_update = true;
                                continue;
                            }

                            // Check for bracket skip-over (typing closing bracket when already there)
                            if brackets.auto_close {
                                let is_closing_bracket =
                                    brackets.pairs.iter().any(|(_, close)| *close == c);
                                if is_closing_bracket && should_skip_auto_close(&state, c) {
                                    // Just move cursor past the existing bracket
                                    state.move_cursor(1);
                                    tv.pending_update = true;
                                    continue;
                                }
                            }

                            insert_char(&mut state, c);

                            // Auto-close brackets
                            if brackets.auto_close {
                                if let Some(closing) = get_closing_bracket(c, &brackets.pairs) {
                                    insert_closing_char(&mut state, closing);
                                }
                            }

                            // Auto-close quotes
                            if brackets.auto_close_quotes {
                                if let Some(closing) = get_closing_quote(c) {
                                    // Only auto-close if we didn't just skip over an existing quote
                                    // and if the previous char wasn't an alphanumeric (to avoid closing in contractions like "don't")
                                    let should_close = if c == '\'' {
                                        // For single quotes, check if previous char is alphanumeric
                                        let cursor = state.cursor_pos;
                                        if cursor >= 2 {
                                            let prev_char = tv.rope.char(cursor - 2);
                                            !prev_char.is_alphanumeric()
                                        } else {
                                            true
                                        }
                                    } else {
                                        true
                                    };

                                    if should_close {
                                        insert_closing_char(&mut state, closing);
                                    }
                                }
                            }

                            // Notify LSP of text change
                            #[cfg(feature = "lsp")]
                            send_did_change(&state, &lsp_client, &mut lsp_sync);

                            // Auto-trigger completion on trigger chars, OR update filter if already visible
                            #[cfg(feature = "lsp")]
                            if lsp.completion.enabled {
                                // Check for trigger characters (including multi-char like "::")
                                let mut is_trigger = false;
                                let cursor_pos = state.cursor_pos;

                                for trigger in &lsp.completion.trigger_characters {
                                    if trigger.len() == 1 {
                                        // Single character trigger (like ".")
                                        if c.to_string() == *trigger {
                                            is_trigger = true;
                                            #[cfg(debug_assertions)]
                                            eprintln!(
                                                "[LSP] Single-char trigger detected: '{}'",
                                                trigger
                                            );
                                            break;
                                        }
                                    } else {
                                        // Multi-character trigger (like "::")
                                        // Check if we just completed typing this trigger
                                        if cursor_pos >= trigger.len() {
                                            let start = cursor_pos - trigger.len();
                                            let recent_text: String = tv
                                                .rope
                                                .slice(start..cursor_pos)
                                                .chars()
                                                .collect();
                                            #[cfg(debug_assertions)]
                                            debug!("[LSP] Checking multi-char trigger '{}' against recent text '{}'", trigger, recent_text);
                                            if recent_text == *trigger {
                                                is_trigger = true;
                                                #[cfg(debug_assertions)]
                                                eprintln!(
                                                    "[LSP] Multi-char trigger MATCHED: '{}'",
                                                    trigger
                                                );
                                                break;
                                            }
                                        }
                                    }
                                }

                                if is_trigger {
                                    // Trigger character detected - open new completion
                                    // Mark completion as not visible to force start_char_index reset
                                    #[cfg(debug_assertions)]
                                    debug!("[LSP] Trigger detected, requesting completion, was_visible={}", completion_state.visible);
                                    completion_state.visible = false;
                                    request_completion(
                                        &state,
                                        &lsp_client,
                                        &mut completion_state,
                                        &lsp_sync,
                                    );
                                } else if c.is_alphanumeric() || c == '_' {
                                    // Typing identifier characters
                                    if completion_state.visible {
                                        // Completion already visible - update filter
                                        #[cfg(debug_assertions)]
                                        debug!("[LSP] Completion visible, updating filter for char '{}'", c);
                                        update_completion_filter(&state, &mut completion_state);
                                    } else {
                                        // Not visible yet - check if we should auto-trigger after N chars
                                        let word_start =
                                            find_word_start(&tv.rope, state.cursor_pos);
                                        let word_len = state.cursor_pos - word_start;

                                        // Trigger after min_word_length characters (configurable, like VSCode's 3)
                                        if word_len >= lsp.completion.min_word_length {
                                            // Set start_char_index to word start so filter works correctly
                                            completion_state.start_char_index = word_start;
                                            request_completion(
                                                &state,
                                                &lsp_client,
                                                &mut completion_state,
                                                &lsp_sync,
                                            );
                                        }
                                    }
                                } else if completion_state.visible {
                                    // Non-identifier, non-trigger character while completion is visible - dismiss it
                                    #[cfg(debug_assertions)]
                                    debug!("[LSP] Non-identifier char '{}' while completion visible, dismissing", c);
                                    completion_state.visible = false;
                                    completion_state.filter.clear();
                                }
                            }
                        }
                    }
                    // Bevy sends Space as a separate variant, not Character(" ")
                    bevy::input::keyboard::Key::Space => {
                        insert_char(&mut state, ' ');
                        // Notify LSP of text change
                        #[cfg(feature = "lsp")]
                        send_did_change(&state, &lsp_client, &mut lsp_sync);
                        // Dismiss completion on space
                        #[cfg(feature = "lsp")]
                        {
                            completion_state.visible = false;
                        }
                    }
                    _ => {}
                }
            }
        }
    } else {
        // Drain character events when a keybinding was triggered
        // This prevents the character from being inserted
        for _ in char_events.read() {}
    }

    // Execute the action if we have one
    if let Some(action) = action_to_execute {
        // Handle Save action - emit event for host app
        if action == EditorAction::Save {
            let content: String = tv.rope.chars().collect();
            save_events.write(crate::types::SaveRequested { content });
            return;
        }

        // Handle Open action - emit event for host app
        if action == EditorAction::Open {
            open_events.write(crate::types::OpenRequested);
            return;
        }

        // Handle RenameSymbol specially (LSP feature)
        #[cfg(feature = "lsp")]
        if action == EditorAction::RenameSymbol {
            if lsp_client.capabilities.supports_rename() {
                if let Some(uri) = &lsp_sync.document_uri {
                    // Convert cursor position to LSP position
                    let cursor_pos = state.cursor_pos.min(tv.rope.len_chars());
                    let line = tv.rope.char_to_line(cursor_pos);
                    let line_start = tv.rope.line_to_char(line);
                    let character = cursor_pos - line_start;

                    let position = lsp_types::Position {
                        line: line as u32,
                        character: character as u32,
                    };

                    // Start prepare rename flow
                    rename_state.start_prepare(position);
                    crate::lsp::systems::request_prepare_rename(&lsp_client, uri, position);
                }
            }
            return;
        }

        #[cfg(all(not(feature = "lsp"), feature = "folding"))]
        execute_action(
            &mut state,
            action,
            &indentation,
            &mut goto_line_state,
            &mut fold_state,
        );
        #[cfg(all(not(feature = "lsp"), not(feature = "folding")))]
        execute_action(
            &mut state,
            action,
            &indentation,
            &mut goto_line_state,
        );
        #[cfg(all(feature = "lsp", feature = "folding"))]
        execute_action(
            &mut state,
            action,
            &indentation,
            &lsp,
            &mut goto_line_state,
            &mut fold_state,
            &lsp_client,
            &mut completion_state,
            &mut lsp_sync,
        );
        #[cfg(all(feature = "lsp", not(feature = "folding")))]
        execute_action(
            &mut state,
            action,
            &indentation,
            &lsp,
            &mut goto_line_state,
            &lsp_client,
            &mut completion_state,
            &mut lsp_sync,
        );
    }
}
