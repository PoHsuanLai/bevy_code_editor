//! Signature help: drain responses and helper to request signature help.

use bevy::prelude::*;
use lsp_types::*;

use crate::types::CodeEditor;

use super::super::state::{LspSignatureHelpPopup, SignatureLifecycle};
use bevy_lsp::{LspMessage, LspRequest, LspSignatureHelpResponse, ServerCapabilities};

pub fn on_lsp_signature_help(
    mut events: MessageReader<LspSignatureHelpResponse>,
    mut q: Query<(&mut LspSignatureHelpPopup, &mut SignatureLifecycle), With<CodeEditor>>,
) {
    for ev in events.read() {
        let Ok((mut sig_state, mut sig_lc)) = q.get_mut(ev.entity) else {
            continue;
        };
        #[cfg(debug_assertions)]
        debug!(
            "[LSP] SignatureHelp(id={}): {} signature(s)",
            ev.id,
            ev.signatures.len()
        );
        if !sig_lc.accept_response(ev.id) {
            continue;
        }
        sig_state.signatures = ev.signatures.clone();
        sig_state.active_signature = ev.active_signature.unwrap_or(0) as usize;
        sig_state.active_parameter = ev.active_parameter.unwrap_or(0) as usize;
        sig_state.visible = !sig_state.signatures.is_empty();
    }
}

/// Helper to send signature help request. The id-bump lives on
/// [`SignatureLifecycle`] so the response handler can drop stale results.
pub fn request_signature_help(
    entity: Entity,
    capabilities: &ServerCapabilities,
    uri: &Url,
    position: Position,
    sig_lc: &mut SignatureLifecycle,
    lsp_w: &mut MessageWriter<LspRequest>,
    session: Option<&crate::lsp_ui::session::LspSession>,
) {
    if capabilities.supports_signature_help() {
        let id = sig_lc.new_request();
        lsp_w.write(crate::lsp_ui::session::lsp_request(
            entity,
            session,
            LspMessage::SignatureHelp {
                uri: uri.clone(),
                position,
                id,
            },
        ));
    }
}
