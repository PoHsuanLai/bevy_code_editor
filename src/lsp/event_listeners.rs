//! LSP event listener systems
//!
//! These systems listen to editor events and translate them into LSP operations

use crate::events::{
    ApplyCompletionEvent, DismissCompletionEvent, RequestCompletionEvent, RequestHoverEvent,
    RequestRenameEvent, RequestSignatureHelpEvent, TextEditEvent,
};
use crate::lsp::client::LspClient;
use crate::lsp::messages::LspMessage;
use crate::lsp::state::{
    CompletionState, LspDebounceTimers, LspSyncState, PendingLspRequest, RenameState,
    SignatureHelpState,
};
use crate::text_view::TextViewState;
use crate::types::{CodeEditor, CodeEditorState, CursorState};
use bevy::prelude::*;

/// System that listens to TextEditEvent and sends incremental didChange to LSP.
///
/// Converts byte-level edit ranges to LSP Position (line/character) and sends
/// only the changed range instead of the full document text.
pub fn listen_text_edit_events(
    mut events: MessageReader<TextEditEvent>,
    query: Query<&TextViewState, With<CodeEditor>>,
    lsp_client: Res<LspClient>,
    mut lsp_sync: ResMut<LspSyncState>,
) {
    let Ok(tv) = query.single() else { return };
    for event in events.read() {
        if let Some(uri) = lsp_sync.document_uri.clone() {
            lsp_sync.document_version += 1;

            // Get the full text content
            let text = tv.rope.to_string();

            use lsp_types::TextDocumentContentChangeEvent;
            let msg = LspMessage::DidChange {
                uri,
                version: lsp_sync.document_version,
                changes: vec![TextDocumentContentChangeEvent {
                    range: Some(lsp_types::Range {
                        start: start_pos,
                        end: start_pos, // Will be overridden by range_length
                    }),
                    range_length: Some(old_len as u32),
                    text: new_text,
                }],
            };

            lsp_client.send(msg);
        }
    }
}

/// Convert a byte offset to LSP Position (line, character) using the rope
fn byte_to_lsp_position(rope: &ropey::Rope, byte_offset: usize) -> lsp_types::Position {
    let byte_offset = byte_offset.min(rope.len_bytes());
    let char_offset = rope.byte_to_char(byte_offset);
    let line = rope.char_to_line(char_offset);
    let line_start_char = rope.line_to_char(line);
    let character = char_offset - line_start_char;
    lsp_types::Position {
        line: line as u32,
        character: character as u32,
    }
}

/// System that listens to RequestCompletionEvent and buffers for debouncing
pub fn listen_completion_requests(
    mut events: MessageReader<RequestCompletionEvent>,
    lsp_client: Res<LspClient>,
    lsp_sync: Res<LspSyncState>,
    mut debounce: ResMut<LspDebounceTimers>,
    mut completion_state: ResMut<CompletionState>,
) {
    for event in events.read() {
        if let Some(uri) = &lsp_sync.document_uri {
            // Buffer the request and reset the debounce timer
            debounce.pending_completion = Some(PendingLspRequest {
                uri: uri.clone(),
                line: event.line as u32,
                character: event.character as u32,
            });
            debounce.completion_timer.reset();

            // Mark completion as pending
            completion_state.visible = true;
        }
    }
}

/// System that listens to RequestHoverEvent and buffers for debouncing
pub fn listen_hover_requests(
    mut events: MessageReader<RequestHoverEvent>,
    lsp_sync: Res<LspSyncState>,
    mut debounce: ResMut<LspDebounceTimers>,
) {
    for event in events.read() {
        if let Some(uri) = &lsp_sync.document_uri {
            // Buffer the request and reset the debounce timer
            debounce.pending_hover = Some(PendingLspRequest {
                uri: uri.clone(),
                line: event.line as u32,
                character: event.character as u32,
            });
            debounce.hover_timer.reset();
        }
    }
}

/// System that listens to RequestRenameEvent
pub fn listen_rename_requests(
    mut events: MessageReader<RequestRenameEvent>,
    lsp_sync: Res<LspSyncState>,
    lsp_client: Res<LspClient>,
    mut rename_state: ResMut<RenameState>,
) {
    for event in events.read() {
        if let Some(uri) = &lsp_sync.document_uri {
            use lsp_types::Position;
            let position = Position {
                line: event.line as u32,
                character: event.character as u32,
            };
            rename_state.start_prepare(position);
            lsp_client.send(LspMessage::PrepareRename {
                uri: uri.clone(),
                position,
            });
        }
    }
}

/// System that listens to RequestSignatureHelpEvent
pub fn listen_signature_help_requests(
    mut events: MessageReader<RequestSignatureHelpEvent>,
    lsp_sync: Res<LspSyncState>,
    lsp_client: Res<LspClient>,
    mut sig_help_state: ResMut<SignatureHelpState>,
) {
    for event in events.read() {
        if let Some(uri) = &lsp_sync.document_uri {
            sig_help_state.reset();
            use lsp_types::Position;
            let msg = LspMessage::SignatureHelp {
                uri: uri.clone(),
                position: Position {
                    line: event.line as u32,
                    character: event.character as u32,
                },
            };

            lsp_client.send(msg);
        }
    }
}

/// System that listens to DismissCompletionEvent
pub fn listen_dismiss_completion(
    mut events: MessageReader<DismissCompletionEvent>,
    mut completion_state: ResMut<CompletionState>,
) {
    for _ in events.read() {
        completion_state.visible = false;
        completion_state.items.clear();
        completion_state.selected_index = 0;
    }
}

/// System that ticks debounce timers and sends LSP requests when they fire
pub fn tick_lsp_debounce_timers(
    time: Res<Time>,
    mut debounce: ResMut<LspDebounceTimers>,
    lsp_client: Res<LspClient>,
) {
    // Tick all active timers
    if debounce.pending_completion.is_some() {
        debounce.completion_timer.tick(time.delta());
        if debounce.completion_timer.just_finished() {
            if let Some(req) = debounce.pending_completion.take() {
                lsp_client.send(LspMessage::Completion {
                    uri: req.uri,
                    position: lsp_types::Position {
                        line: req.line,
                        character: req.character,
                    },
                });
            }
        }
    }

    if debounce.pending_hover.is_some() {
        debounce.hover_timer.tick(time.delta());
        if debounce.hover_timer.just_finished() {
            if let Some(req) = debounce.pending_hover.take() {
                lsp_client.send(LspMessage::Hover {
                    uri: req.uri,
                    position: lsp_types::Position {
                        line: req.line,
                        character: req.character,
                    },
                });
            }
        }
    }

    if debounce.pending_highlight.is_some() {
        debounce.highlight_timer.tick(time.delta());
        if debounce.highlight_timer.just_finished() {
            if let Some(req) = debounce.pending_highlight.take() {
                lsp_client.send(LspMessage::DocumentHighlight {
                    uri: req.uri,
                    position: lsp_types::Position {
                        line: req.line,
                        character: req.character,
                    },
                });
            }
        }
    }

    if debounce.pending_code_action.is_some() {
        debounce.code_action_timer.tick(time.delta());
        if debounce.code_action_timer.just_finished() {
            if let Some(req) = debounce.pending_code_action.take() {
                lsp_client.send(LspMessage::CodeAction {
                    uri: req.uri,
                    range: req.range,
                    diagnostics: Vec::new(),
                });
            }
        }
    }
}

/// System that listens to ApplyCompletionEvent
pub fn listen_apply_completion(
    mut events: MessageReader<ApplyCompletionEvent>,
    mut query: Query<(&mut CodeEditorState, &mut CursorState, &mut TextViewState), With<CodeEditor>>,
    mut completion_state: ResMut<CompletionState>,
) {
    let Ok((mut editor, mut cursor_state, mut tv)) = query.single_mut() else { return };
    for event in events.read() {
        if event.item_index < completion_state.items.len() {
            let item = &completion_state.items[event.item_index];

            // Calculate current position from cursor_pos
            let cursor_pos = cursor_state.cursor_pos.min(tv.rope.len_chars());
            let line = tv.rope.char_to_line(cursor_pos);
            let line_start = tv.rope.line_to_char(line);
            let cursor_char = cursor_pos - line_start;

            let line_text = tv.rope.line(line).to_string();

            let word_start = line_text[..cursor_char]
                .rfind(|c: char| !c.is_alphanumeric() && c != '_')
                .map(|i| i + 1)
                .unwrap_or(0);

            // Delete the partial word
            if word_start < cursor_char {
                let start_pos = line_start + word_start;
                let end_pos = line_start + cursor_char;
                tv.rope.remove(start_pos..end_pos);
                cursor_state.cursor_pos = start_pos + word_start;
            }

            // Insert the completion text
            let insert_text = item.insert_text.as_ref().unwrap_or(&item.label);
            let cursor_pos = cursor_state.cursor_pos;
            tv.rope.insert(cursor_pos, insert_text);
            cursor_state.cursor_pos += insert_text.len();

            // Mark as needing update
            tv.pending_update = true;
            tv.content_version += 1;

            // Dismiss completion
            completion_state.visible = false;
            completion_state.items.clear();
        }
    }
}
