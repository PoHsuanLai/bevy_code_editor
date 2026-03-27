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

pub fn handle_keyboard_input(
    mut editor_query: Query<(&mut CodeEditorState, &mut SelectionState, &mut EditHistoryState, &mut SyntaxCacheState, &mut EditorDisplayState, &mut CursorState, &mut TextViewState, &TextViewViewport), With<CodeEditor>>,
    mut char_events: MessageReader<KeyboardInput>,
    action_query: Query<&ActionState<EditorAction>, With<EditorInputManager>>,
    cursor_settings: Res<CursorSettings>,
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
    let Ok((mut state, mut sel, mut hist, mut syntax, mut display, mut cursor, mut tv, _viewport)) = editor_query.single_mut() else { return; };

    if !state.is_focused {
        return;
    }

    let Ok(action_state) = action_query.single() else {
        warn!("No EditorInputManager entity found with ActionState");
        return;
    };

    #[cfg(feature = "lsp")]
    if rename_state.visible {
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
                    rename_state.reset();
                }
                _ => {}
            }
        }
        return;
    }

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

    // Folding actions checked separately to keep ALL_ACTIONS array small
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

    // Key repeat on held actions
    if action_to_execute.is_none() {
        if let Some(current_action) = key_repeat_state.current_action {
            if action_state.pressed(&current_action) {
                let initial_delay = cursor_settings.key_repeat.initial_delay_ms as f64 / 1000.0;
                let repeat_interval = cursor_settings.key_repeat.repeat_delay_ms as f64 / 1000.0;

                if let Some(press_start) = key_repeat_state.press_start {
                    let elapsed = now.duration_since(press_start).as_secs_f64();

                    if elapsed >= initial_delay {
                        let should_repeat = match key_repeat_state.last_repeat {
                            Some(last) => now.duration_since(last).as_secs_f64() >= repeat_interval,
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

    if action_to_execute.is_none() {
        for event in char_events.read() {
            if event.state.is_pressed() {
                match &event.logical_key {
                    bevy::input::keyboard::Key::Character(ref text) => {
                        for c in text.chars() {
                            if c.is_control() {
                                continue;
                            }

                            // Skip over existing closing quote instead of inserting a duplicate
                            if brackets.auto_close_quotes
                                && get_closing_quote(c).is_some()
                                && should_skip_auto_close(&cursor, &tv.rope, c)
                            {
                                state.move_cursor(&mut cursor, &tv.rope, 1);
                                tv.pending_update = true;
                                continue;
                            }

                            // Skip over existing closing bracket instead of inserting a duplicate
                            if brackets.auto_close {
                                let is_closing_bracket =
                                    brackets.pairs.iter().any(|(_, close)| *close == c);
                                if is_closing_bracket && should_skip_auto_close(&cursor, &tv.rope, c) {
                                    state.move_cursor(&mut cursor, &tv.rope, 1);
                                    tv.pending_update = true;
                                    continue;
                                }
                            }

                            insert_char(&mut state, &mut sel, &mut hist, &mut syntax, &mut display, &mut cursor, &mut tv, c);

                            if brackets.auto_close {
                                if let Some(closing) = get_closing_bracket(c, &brackets.pairs) {
                                    insert_closing_char(&mut state, &cursor, &mut tv, closing);
                                }
                            }

                            if brackets.auto_close_quotes {
                                if let Some(closing) = get_closing_quote(c) {
                                    // Don't auto-close single quotes mid-word (e.g. "don't")
                                    let should_close = if c == '\'' {
                                        let cur_pos = cursor.cursor_pos;
                                        if cur_pos >= 2 {
                                            let prev_char = tv.rope.char(cur_pos - 2);
                                            !prev_char.is_alphanumeric()
                                        } else {
                                            true
                                        }
                                    } else {
                                        true
                                    };

                                    if should_close {
                                        insert_closing_char(&mut state, &cursor, &mut tv, closing);
                                    }
                                }
                            }

                            #[cfg(feature = "lsp")]
                            send_did_change(&tv.rope, &lsp_client, &mut lsp_sync);

                            // Trigger or update LSP completion based on typed character
                            #[cfg(feature = "lsp")]
                            if lsp.completion.enabled {
                                let mut is_trigger = false;
                                let cursor_pos = cursor.cursor_pos;

                                for trigger in &lsp.completion.trigger_characters {
                                    if trigger.len() == 1 {
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
                                        // Multi-character trigger (e.g. "::")
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
                                    // Force start_char_index reset by marking not visible
                                    #[cfg(debug_assertions)]
                                    debug!("[LSP] Trigger detected, requesting completion, was_visible={}", completion_state.visible);
                                    completion_state.visible = false;
                                    request_completion(
                                        &cursor,
                                        &tv.rope,
                                        &lsp_client,
                                        &mut completion_state,
                                        &lsp_sync,
                                    );
                                } else if c.is_alphanumeric() || c == '_'  {
                                    if completion_state.visible {
                                        #[cfg(debug_assertions)]
                                        eprintln!("[LSP] Completion visible, updating filter for char '{}'", c);
                                        update_completion_filter(&cursor, &tv.rope, &mut completion_state);
                                    } else {
                                        // Auto-trigger after min_word_length identifier chars
                                        let word_start =
                                            find_word_start(&tv.rope, cursor.cursor_pos);
                                        let word_len = cursor.cursor_pos - word_start;

                                        if word_len >= lsp.completion.min_word_length {
                                            completion_state.start_char_index = word_start;
                                            request_completion(
                                                &cursor,
                                                &tv.rope,
                                                &lsp_client,
                                                &mut completion_state,
                                                &lsp_sync,
                                            );
                                        }
                                    }
                                } else if completion_state.visible {
                                    #[cfg(debug_assertions)]
                                    debug!("[LSP] Non-identifier char '{}' while completion visible, dismissing", c);
                                    completion_state.visible = false;
                                    completion_state.filter.clear();
                                }
                            }
                        }
                    }
                    bevy::input::keyboard::Key::Space => {
                        insert_char(&mut state, &mut sel, &mut hist, &mut syntax, &mut display, &mut cursor, &mut tv, ' ');
                        #[cfg(feature = "lsp")]
                        send_did_change(&tv.rope, &lsp_client, &mut lsp_sync);
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
        // Drain char events so they don't get inserted alongside the keybinding action
        for _ in char_events.read() {}
    }

    if let Some(action) = action_to_execute {
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

                    eprintln!(
                        "[Rename] Requesting prepare rename at line={}, char={}",
                        position.line, position.character
                    );

                    rename_state.start_prepare(position);
                    crate::lsp::systems::request_prepare_rename(&lsp_client, uri, position);
                }
            }
            return;
        }

        execute_action(
            &mut state, &mut sel, &mut hist, &mut syntax, &mut display,
            &mut cursor, &mut tv, action, &indentation,
            #[cfg(feature = "lsp")] &lsp,
            &mut goto_line_state,
            #[cfg(feature = "folding")] &mut fold_state,
            #[cfg(feature = "lsp")] &lsp_client,
            #[cfg(feature = "lsp")] &mut completion_state,
            #[cfg(feature = "lsp")] &mut lsp_sync,
        );
    }
}
