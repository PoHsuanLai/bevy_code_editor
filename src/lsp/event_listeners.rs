//! LSP event listener systems
//!
//! These systems listen to editor events and translate them into LSP operations

use bevy::prelude::*;
use crate::events::{
    ApplyCompletionEvent, DismissCompletionEvent, RequestCompletionEvent, RequestHoverEvent,
    RequestRenameEvent, RequestSignatureHelpEvent, TextEditEvent,
};
use crate::lsp::client::LspClient;
use crate::lsp::messages::LspMessage;
use crate::lsp::state::{CompletionState, HoverState, LspSyncState, RenameState, SignatureHelpState};
use crate::types::CodeEditorState;

/// System that listens to TextEditEvent and sends didChange to LSP
pub fn listen_text_edit_events(
    mut events: MessageReader<TextEditEvent>,
    state: Res<CodeEditorState>,
    lsp_client: Res<LspClient>,
    mut lsp_sync: ResMut<LspSyncState>,
) {
    for event in events.read() {
        // Only send if we have a document URI
        if let Some(uri) = lsp_sync.document_uri.clone() {
            // Increment document version
            lsp_sync.document_version += 1;

            // Get the full text content
            let text = state.rope.to_string();

            // Send didChange notification
            use lsp_types::TextDocumentContentChangeEvent;
            let msg = LspMessage::DidChange {
                uri,
                version: lsp_sync.document_version,
                changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text,
                }],
            };

            lsp_client.send(msg);
        }
    }
}

/// System that listens to RequestCompletionEvent
pub fn listen_completion_requests(
    mut events: MessageReader<RequestCompletionEvent>,
    state: Res<CodeEditorState>,
    lsp_client: Res<LspClient>,
    lsp_sync: Res<LspSyncState>,
    mut completion_state: ResMut<CompletionState>,
) {
    for event in events.read() {
        if let Some(uri) = &lsp_sync.document_uri {
            // Send completion request
            use lsp_types::Position;
            let msg = LspMessage::Completion {
                uri: uri.clone(),
                position: Position {
                    line: event.line as u32,
                    character: event.character as u32,
                },
            };

            lsp_client.send(msg);

            // Mark completion as pending
            completion_state.visible = true;
        }
    }
}

/// System that listens to RequestHoverEvent
pub fn listen_hover_requests(
    mut events: MessageReader<RequestHoverEvent>,
    lsp_sync: Res<LspSyncState>,
    lsp_client: Res<LspClient>,
    mut hover_state: ResMut<HoverState>,
) {
    for event in events.read() {
        if let Some(uri) = &lsp_sync.document_uri {
            // Send hover request
            use lsp_types::Position;
            let msg = LspMessage::Hover {
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

/// System that listens to RequestRenameEvent
pub fn listen_rename_requests(
    mut events: MessageReader<RequestRenameEvent>,
    lsp_sync: Res<LspSyncState>,
    lsp_client: Res<LspClient>,
    mut rename_state: ResMut<RenameState>,
) {
    for event in events.read() {
        if let Some(uri) = &lsp_sync.document_uri {
            // Send prepareRename request
            use lsp_types::Position;
            let msg = LspMessage::PrepareRename {
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

/// System that listens to RequestSignatureHelpEvent
pub fn listen_signature_help_requests(
    mut events: MessageReader<RequestSignatureHelpEvent>,
    lsp_sync: Res<LspSyncState>,
    lsp_client: Res<LspClient>,
    mut sig_help_state: ResMut<SignatureHelpState>,
) {
    for event in events.read() {
        if let Some(uri) = &lsp_sync.document_uri {
            // Send signatureHelp request
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

/// System that listens to ApplyCompletionEvent
pub fn listen_apply_completion(
    mut events: MessageReader<ApplyCompletionEvent>,
    mut state: ResMut<CodeEditorState>,
    mut completion_state: ResMut<CompletionState>,
) {
    for event in events.read() {
        if event.item_index < completion_state.items.len() {
            let item = &completion_state.items[event.item_index];

            // Calculate current position from cursor_pos
            let cursor_pos = state.cursor_pos.min(state.rope.len_chars());
            let line = state.rope.char_to_line(cursor_pos);
            let line_start = state.rope.line_to_char(line);
            let cursor_char = cursor_pos - line_start;

            let line_text = state.rope.line(line).to_string();

            let word_start = line_text[..cursor_char]
                .rfind(|c: char| !c.is_alphanumeric() && c != '_')
                .map(|i| i + 1)
                .unwrap_or(0);

            // Delete the partial word
            if word_start < cursor_char {
                let start_pos = line_start + word_start;
                let end_pos = line_start + cursor_char;
                state.rope.remove(start_pos..end_pos);
                state.cursor_pos = start_pos + word_start;
            }

            // Insert the completion text
            let insert_text = item.insert_text.as_ref().unwrap_or(&item.label);
            let cursor_pos = state.cursor_pos;
            state.rope.insert(cursor_pos, insert_text);
            state.cursor_pos += insert_text.len();

            // Mark as needing update
            state.pending_update = true;
            state.content_version += 1;

            // Dismiss completion
            completion_state.visible = false;
            completion_state.items.clear();
        }
    }
}
