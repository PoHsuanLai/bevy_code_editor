//! LSP event listener systems.
//!
//! These systems read editor events ([`crate::types::events::*`]) and translate
//! them into LSP request sends through each editor entity's
//! [`bevy_lsp::LspClient`] Component.
//!
//! Position conversion goes through [`bevy_lsp::rope_char_to_lsp_position`]
//! with [`PositionEncoding::Utf16`] (LSP spec default). Once we read the
//! server's negotiated `position_encoding` from `initialize.serverInfo.
//! capabilities` we'll thread that through here instead.

use super::state::{
    LspCompletionPopup, LspDebounceTimers, LspRenamePopup, LspSignatureHelpPopup, PendingLspRequest,
};
use crate::text_view::TextViewState;
use crate::types::events::{
    ApplyCompletionEvent, DismissCompletionEvent, RequestCompletionEvent, RequestHoverEvent,
    RequestRenameEvent, RequestSignatureHelpEvent, TextEditEvent,
};
use crate::types::{CodeEditor, CursorState};
use bevy::prelude::*;
use bevy_lsp::{
    rope_byte_to_lsp_position, rope_char_to_lsp_position, LspClient, LspDocument, LspMessage,
    PositionEncoding,
};

/// LSP wire encoding the editor currently uses. Matches the spec default.
/// TODO: read this off `ServerCapabilities::position_encoding` once it's
/// surfaced as a Component field.
const ENC: PositionEncoding = PositionEncoding::Utf16;

pub fn listen_text_edit_events(
    mut events: MessageReader<TextEditEvent>,
    mut query: Query<(&TextViewState, &LspClient, Option<&mut LspDocument>), With<CodeEditor>>,
) {
    let Ok((tv, lsp_client, lsp_document)) = query.single_mut() else {
        return;
    };
    let Some(mut lsp_document) = lsp_document else {
        return;
    };
    let rope = &tv.rope;
    for event in events.read() {
        let uri = lsp_document.uri.clone();
        let version = lsp_document.bump_version();

        let start_pos = rope_byte_to_lsp_position(rope, event.start_byte, ENC);
        let old_len = event.old_end_byte - event.start_byte;

        let new_text_start = event.start_byte.min(rope.len_bytes());
        let new_text_end = event.new_end_byte.min(rope.len_bytes());
        let new_text = if new_text_start < new_text_end {
            let s = rope.byte_to_char(new_text_start);
            let e = rope.byte_to_char(new_text_end);
            rope.slice(s..e).to_string()
        } else {
            String::new()
        };

        use lsp_types::TextDocumentContentChangeEvent;
        lsp_client.send(LspMessage::DidChange {
            uri,
            version,
            changes: vec![TextDocumentContentChangeEvent {
                range: Some(lsp_types::Range {
                    start: start_pos,
                    end: start_pos, // overridden by range_length
                }),
                range_length: Some(old_len as u32),
                text: new_text,
            }],
        });
    }
}

pub fn listen_completion_requests(
    mut events: MessageReader<RequestCompletionEvent>,
    mut query: Query<
        (
            &TextViewState,
            Option<&LspDocument>,
            &mut LspDebounceTimers,
            &mut LspCompletionPopup,
        ),
        With<CodeEditor>,
    >,
) {
    let Ok((tv, lsp_document, mut debounce, mut completion_state)) = query.single_mut() else {
        return;
    };
    let Some(lsp_document) = lsp_document else {
        return;
    };
    for event in events.read() {
        debounce.pending_completion = Some(PendingLspRequest {
            uri: lsp_document.uri.clone(),
            position: rope_char_to_lsp_position(&tv.rope, event.cursor_char, ENC),
        });
        debounce.completion_timer.reset();
        completion_state.visible = true;
    }
}

pub fn listen_hover_requests(
    mut events: MessageReader<RequestHoverEvent>,
    mut query: Query<(&TextViewState, Option<&LspDocument>, &mut LspDebounceTimers), With<CodeEditor>>,
) {
    let Ok((tv, lsp_document, mut debounce)) = query.single_mut() else {
        return;
    };
    let Some(lsp_document) = lsp_document else {
        return;
    };
    for event in events.read() {
        debounce.pending_hover = Some(PendingLspRequest {
            uri: lsp_document.uri.clone(),
            position: rope_char_to_lsp_position(&tv.rope, event.cursor_char, ENC),
        });
        debounce.hover_timer.reset();
    }
}

pub fn listen_rename_requests(
    mut events: MessageReader<RequestRenameEvent>,
    mut query: Query<
        (
            &TextViewState,
            Option<&LspDocument>,
            &LspClient,
            &mut LspRenamePopup,
        ),
        With<CodeEditor>,
    >,
) {
    let Ok((tv, lsp_document, lsp_client, mut rename_state)) = query.single_mut() else {
        return;
    };
    let Some(lsp_document) = lsp_document else {
        return;
    };
    for event in events.read() {
        let position = rope_char_to_lsp_position(&tv.rope, event.cursor_char, ENC);
        rename_state.start_prepare(position);
        lsp_client.send(LspMessage::PrepareRename {
            uri: lsp_document.uri.clone(),
            position,
        });
    }
}

pub fn listen_signature_help_requests(
    mut events: MessageReader<RequestSignatureHelpEvent>,
    mut query: Query<
        (
            &TextViewState,
            Option<&LspDocument>,
            &LspClient,
            &mut LspSignatureHelpPopup,
        ),
        With<CodeEditor>,
    >,
) {
    let Ok((tv, lsp_document, lsp_client, mut sig_help_state)) = query.single_mut() else {
        return;
    };
    let Some(lsp_document) = lsp_document else {
        return;
    };
    for event in events.read() {
        sig_help_state.reset();
        lsp_client.send(LspMessage::SignatureHelp {
            uri: lsp_document.uri.clone(),
            position: rope_char_to_lsp_position(&tv.rope, event.cursor_char, ENC),
        });
    }
}

pub fn listen_dismiss_completion(
    mut events: MessageReader<DismissCompletionEvent>,
    mut query: Query<&mut LspCompletionPopup, With<CodeEditor>>,
) {
    let Ok(mut completion_state) = query.single_mut() else {
        return;
    };
    for _ in events.read() {
        completion_state.visible = false;
        completion_state.items.clear();
        completion_state.selected_index = 0;
    }
}

/// Tick debounce timers and fire LSP requests when they expire.
///
/// Kept as a free function (not in this file's `pub use` set) and named
/// `tick_lsp_debounce_timers` so callers in `plugin/lsp_plugin.rs` find it
/// at the same path as before the refactor.
pub fn tick_lsp_debounce_timers(
    time: Res<Time>,
    mut query: Query<(&LspClient, &mut LspDebounceTimers), With<CodeEditor>>,
) {
    let Ok((lsp_client, mut debounce)) = query.single_mut() else {
        return;
    };

    if debounce.pending_completion.is_some() {
        debounce.completion_timer.tick(time.delta());
        if debounce.completion_timer.just_finished() {
            if let Some(req) = debounce.pending_completion.take() {
                lsp_client.send(LspMessage::Completion {
                    uri: req.uri,
                    position: req.position,
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
                    position: req.position,
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
                    position: req.position,
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

/// Listens to ApplyCompletionEvent.
pub fn listen_apply_completion(
    mut events: MessageReader<ApplyCompletionEvent>,
    mut query: Query<
        (
            &mut TextViewState,
            &mut CursorState,
            &mut LspCompletionPopup,
        ),
        With<CodeEditor>,
    >,
) {
    let Ok((mut tv, mut cursor_state, mut completion_state)) = query.single_mut() else {
        return;
    };
    for event in events.read() {
        let filtered = completion_state.filtered_items();
        if event.item_index >= filtered.len() {
            continue;
        }
        let item = &filtered[event.item_index];

        let cursor_pos = cursor_state.cursor_pos.min(tv.rope.len_chars());
        let line = tv.rope.char_to_line(cursor_pos);
        let line_start = tv.rope.line_to_char(line);
        let cursor_char = cursor_pos - line_start;

        let start_offset_in_line = if completion_state.start_char_index >= line_start {
            completion_state.start_char_index - line_start
        } else {
            cursor_char
        };
        let start_pos = line_start + start_offset_in_line;
        if start_pos < cursor_pos {
            tv.rope.remove(start_pos..cursor_pos);
            cursor_state.cursor_pos = start_pos;
        }

        let insert_text = item.insert_text();
        let cursor_pos = cursor_state.cursor_pos;
        tv.rope.insert(cursor_pos, insert_text);
        cursor_state.cursor_pos += insert_text.len();
        completion_state.visible = false;
        completion_state.filter.clear();
    }
}
