//! Document highlights: drain responses and request highlights on cursor settle.

use bevy::prelude::*;
use bevy_instanced_text_editor::RopeBuffer;

use crate::text_view::InstancedText;
use crate::types::{CodeEditor, CursorState};
use bevy::input_focus::InputFocus;

use super::super::state::LspDocumentHighlights;
use bevy_lsp::{
    LspDocument, LspDocumentHighlightsResponse, LspMessage, LspRequest, ServerCapabilities,
};

type RequestDocumentHighlightsQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ServerCapabilities,
        &'static CursorState,
        &'static InstancedText<RopeBuffer>,
        Option<&'static LspDocument>,
        &'static mut LspDocumentHighlights,
        &'static crate::settings::LspConfig,
        Option<&'static crate::lsp_ui::session::LspSession>,
    ),
    With<CodeEditor>,
>;

pub fn on_lsp_document_highlights(
    mut events: MessageReader<LspDocumentHighlightsResponse>,
    mut q: Query<&mut LspDocumentHighlights, With<CodeEditor>>,
) {
    for ev in events.read() {
        let Ok(mut highlight_state) = q.get_mut(ev.entity) else {
            continue;
        };
        trace!(
            "[LSP] DocumentHighlights: {} highlight(s)",
            ev.highlights.len()
        );
        highlight_state.highlights = ev.highlights.clone();
        highlight_state.visible = !highlight_state.highlights.is_empty();
        highlight_state.in_flight_position = None;
    }
}

/// Fire `textDocument/documentHighlight` when the cursor settles on a
/// new position. Highlights all occurrences of the symbol under cursor
/// (the IDE feature where clicking on a name highlights every other use
/// in the same file). Debounce delay comes from
/// `LspConfig::highlight_delay_ms`.
pub fn request_document_highlights(
    time: Res<Time>,
    mut query: RequestDocumentHighlightsQuery,
    mut lsp_w: MessageWriter<LspRequest>,
    input_focus: Res<InputFocus>,
) {
    let Some(focused) = input_focus.get() else {
        return;
    };
    let Ok((
        entity,
        capabilities,
        cursor_state,
        buffer,
        lsp_document,
        mut highlight_state,
        settings,
        session,
    )) = query.get_mut(focused)
    else {
        return;
    };
    if !capabilities.supports_document_highlight() {
        return;
    }

    let Some(lsp_document) = lsp_document else {
        return;
    };

    let cursor_pos = cursor_state.cursor_pos;

    if highlight_state.in_flight_position == Some(cursor_pos) {
        return;
    }
    if highlight_state.cursor_position == cursor_pos && highlight_state.visible {
        return;
    }

    if highlight_state.cursor_position != cursor_pos || highlight_state.debounce_timer.is_none() {
        highlight_state.cursor_position = cursor_pos;
        highlight_state.debounce_timer = Some(Timer::new(
            std::time::Duration::from_millis(settings.highlight_delay_ms),
            TimerMode::Once,
        ));
        if highlight_state.visible {
            highlight_state.highlights.clear();
            highlight_state.visible = false;
        }
        return;
    }

    let timer = highlight_state.debounce_timer.as_mut().unwrap();
    timer.tick(time.delta());
    if !timer.is_finished() {
        return;
    }
    highlight_state.debounce_timer = None;
    highlight_state.in_flight_position = Some(cursor_pos);

    let position = bevy_lsp::rope_char_to_lsp_position(
        buffer.rope(),
        cursor_pos,
        capabilities.position_encoding(),
    );
    lsp_w.write(crate::lsp_ui::session::lsp_request(
        entity,
        session,
        LspMessage::DocumentHighlight {
            uri: lsp_document.uri.clone(),
            position,
            id: 0,
        },
    ));
}
