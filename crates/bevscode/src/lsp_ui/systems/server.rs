//! Server lifecycle: initialize, shutdown-ack, and crash handling.

use bevy::prelude::*;

use crate::types::CodeEditor;

use super::super::completion::LspCompletionPopup;
use super::super::state::{
    CodeActionsLifecycle, CompletionLifecycle, HoverLifecycle, LspCodeActionsPopup,
    LspDocumentHighlights, LspHoverPopup, LspRenamePopup, LspSignatureHelpPopup, RenameLifecycle,
    SignatureLifecycle,
};
use bevy_lsp::{LspServerCrashed, LspServerInitialized, LspShutdownAck, ServerCapabilities};

type LspServerCrashedQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut LspCompletionPopup,
        &'static mut LspHoverPopup,
        &'static mut LspSignatureHelpPopup,
        &'static mut LspCodeActionsPopup,
        &'static mut LspDocumentHighlights,
        &'static mut LspRenamePopup,
        &'static mut CompletionLifecycle,
        &'static mut HoverLifecycle,
        &'static mut SignatureLifecycle,
        &'static mut CodeActionsLifecycle,
        &'static mut RenameLifecycle,
    ),
    With<CodeEditor>,
>;

/// Records server capabilities on the editor when the server finishes
/// `initialize`. `LspClient.initialized` is already flipped by `bevy_lsp`'s
/// drain.
pub fn on_lsp_initialized(
    mut events: MessageReader<LspServerInitialized>,
    mut q: Query<&mut ServerCapabilities, With<CodeEditor>>,
) {
    for ev in events.read() {
        let Ok(mut capabilities) = q.get_mut(ev.entity) else {
            continue;
        };
        capabilities.set(ev.capabilities.clone());
        #[cfg(debug_assertions)]
        debug!("[LSP] Server initialized");
    }
}

pub fn on_lsp_shutdown_ack(mut events: MessageReader<LspShutdownAck>) {
    for _ev in events.read() {
        // Caller follows up with `Exit`; nothing else to do here.
        debug!("[LSP] ShutdownAck received");
    }
}

pub fn on_lsp_server_crashed(
    mut events: MessageReader<LspServerCrashed>,
    mut q: LspServerCrashedQuery,
) {
    for ev in events.read() {
        let Ok((
            mut completion_state,
            mut hover_state,
            mut sig_state,
            mut action_state,
            mut highlight_state,
            mut rename_state,
            mut completion_lc,
            mut hover_lc,
            mut sig_lc,
            mut action_lc,
            mut rename_lc,
        )) = q.get_mut(ev.entity)
        else {
            continue;
        };
        warn!("[LSP] server reported crashed / channel closed");
        completion_state.dismiss();
        completion_lc.dismiss();
        hover_state.reset();
        hover_lc.dismiss();
        sig_state.dismiss();
        sig_lc.dismiss();
        action_state.dismiss();
        action_lc.dismiss();
        highlight_state.reset();
        rename_state.reset();
        rename_lc.dismiss();
    }
}
