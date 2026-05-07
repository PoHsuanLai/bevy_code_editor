//! Installs the shared tokio runtime that the async-lsp transport drives on.

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy_tokio_tasks::{TokioTasksPlugin, TokioTasksRuntime};

use crate::client::LspClient;
use crate::messages::{
    LspCodeActionsResponse, LspCompletionResponse, LspDefinitionResponse, LspDiagnosticsUpdated,
    LspDocumentHighlightsResponse, LspFormatResponse, LspHoverResponse, LspInlayHintsResponse,
    LspPrepareRenameResponse, LspReferencesResponse, LspRenameResponse, LspResolvedCompletionItem,
    LspResponse, LspServerCrashed, LspServerInitialized, LspShutdownAck,
    LspSignatureHelpResponse,
};

/// Installs [`TokioTasksPlugin`] (only if the host hasn't already), registers
/// the outbound response messages, drives [`drain_lsp_responses`] each frame,
/// and gracefully shuts down every [`LspClient`] on `AppExit`.
#[derive(Default)]
pub struct LspPlugin;

impl Plugin for LspPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<TokioTasksRuntime>() {
            app.add_plugins(TokioTasksPlugin::default());
        }

        app.add_message::<LspServerInitialized>()
            .add_message::<LspDiagnosticsUpdated>()
            .add_message::<LspCompletionResponse>()
            .add_message::<LspResolvedCompletionItem>()
            .add_message::<LspHoverResponse>()
            .add_message::<LspDefinitionResponse>()
            .add_message::<LspReferencesResponse>()
            .add_message::<LspFormatResponse>()
            .add_message::<LspSignatureHelpResponse>()
            .add_message::<LspCodeActionsResponse>()
            .add_message::<LspInlayHintsResponse>()
            .add_message::<LspDocumentHighlightsResponse>()
            .add_message::<LspPrepareRenameResponse>()
            .add_message::<LspRenameResponse>()
            .add_message::<LspShutdownAck>()
            .add_message::<LspServerCrashed>();

        app.init_resource::<DrainedResponses>();
        app.add_systems(
            Update,
            (
                drain_lsp_responses,
                flush_drained_responses_a.after(drain_lsp_responses),
                flush_drained_responses_b.after(drain_lsp_responses),
            ),
        );
        app.add_systems(Last, shutdown_clients_on_app_exit);
    }
}

/// Buffered drain output: collect fan-out targets first, then write them in
/// `apply_drained_responses`. Splitting the drain (which mutably borrows
/// `LspClient`) from the writers (which mutably borrow message buses) keeps
/// each system under Bevy's 16-parameter cap and lets the scheduler parallelize
/// reader systems against the next drain.
#[derive(Resource, Default)]
struct DrainedResponses {
    initialized: Vec<LspServerInitialized>,
    diagnostics: Vec<LspDiagnosticsUpdated>,
    completion: Vec<LspCompletionResponse>,
    resolved: Vec<LspResolvedCompletionItem>,
    hover: Vec<LspHoverResponse>,
    definition: Vec<LspDefinitionResponse>,
    references: Vec<LspReferencesResponse>,
    format: Vec<LspFormatResponse>,
    signature: Vec<LspSignatureHelpResponse>,
    code_actions: Vec<LspCodeActionsResponse>,
    inlay: Vec<LspInlayHintsResponse>,
    highlights: Vec<LspDocumentHighlightsResponse>,
    prepare_rename: Vec<LspPrepareRenameResponse>,
    rename: Vec<LspRenameResponse>,
    shutdown: Vec<LspShutdownAck>,
    crashed: Vec<LspServerCrashed>,
}

/// Drain the transport channel on every [`LspClient`] into [`DrainedResponses`].
/// Writes to `LspClient.initialized` when it sees `Initialized` so legacy code
/// paths that read the bool keep working.
fn drain_lsp_responses(
    mut clients: Query<(Entity, &mut LspClient)>,
    mut drained: ResMut<DrainedResponses>,
) {
    for (entity, mut client) in clients.iter_mut() {
        client.cleanup_timeouts();
        while let Some(response) = client.try_recv() {
            match response {
                LspResponse::Initialized { capabilities } => {
                    client.initialized = true;
                    drained.initialized.push(LspServerInitialized {
                        entity,
                        capabilities,
                    });
                }
                LspResponse::Diagnostics { uri, diagnostics } => {
                    drained.diagnostics.push(LspDiagnosticsUpdated {
                        entity,
                        uri,
                        diagnostics,
                    });
                }
                LspResponse::Completion {
                    id,
                    items,
                    is_incomplete,
                } => {
                    drained.completion.push(LspCompletionResponse {
                        entity,
                        id,
                        items,
                        is_incomplete,
                    });
                }
                LspResponse::ResolvedCompletionItem { id, item } => {
                    drained
                        .resolved
                        .push(LspResolvedCompletionItem { entity, id, item });
                }
                LspResponse::Hover { content, kind, range } => {
                    drained.hover.push(LspHoverResponse {
                        entity,
                        content,
                        kind,
                        range,
                    });
                }
                LspResponse::Definition { locations } => {
                    drained
                        .definition
                        .push(LspDefinitionResponse { entity, locations });
                }
                LspResponse::References { locations } => {
                    drained
                        .references
                        .push(LspReferencesResponse { entity, locations });
                }
                LspResponse::Format { edits } => {
                    drained.format.push(LspFormatResponse { entity, edits });
                }
                LspResponse::SignatureHelp {
                    id,
                    signatures,
                    active_signature,
                    active_parameter,
                } => {
                    drained.signature.push(LspSignatureHelpResponse {
                        entity,
                        id,
                        signatures,
                        active_signature,
                        active_parameter,
                    });
                }
                LspResponse::CodeActions { id, actions } => {
                    drained.code_actions.push(LspCodeActionsResponse {
                        entity,
                        id,
                        actions,
                    });
                }
                LspResponse::InlayHints { hints } => {
                    drained
                        .inlay
                        .push(LspInlayHintsResponse { entity, hints });
                }
                LspResponse::DocumentHighlights { highlights } => {
                    drained.highlights.push(LspDocumentHighlightsResponse {
                        entity,
                        highlights,
                    });
                }
                LspResponse::PrepareRename { range, placeholder } => {
                    drained.prepare_rename.push(LspPrepareRenameResponse {
                        entity,
                        range,
                        placeholder,
                    });
                }
                LspResponse::Rename { edit } => {
                    drained.rename.push(LspRenameResponse { entity, edit });
                }
                LspResponse::ShutdownAck { id } => {
                    drained.shutdown.push(LspShutdownAck { entity, id });
                }
                LspResponse::Crashed => {
                    drained.crashed.push(LspServerCrashed { entity });
                }
            }
        }
    }
}

/// Flush the per-variant buffers into Bevy's message bus. Two systems —
/// each under the 16-parameter cap — partition the writers so the scheduler
/// stays happy.
fn flush_drained_responses_a(
    mut drained: ResMut<DrainedResponses>,
    mut initialized_w: MessageWriter<LspServerInitialized>,
    mut diagnostics_w: MessageWriter<LspDiagnosticsUpdated>,
    mut completion_w: MessageWriter<LspCompletionResponse>,
    mut resolved_w: MessageWriter<LspResolvedCompletionItem>,
    mut hover_w: MessageWriter<LspHoverResponse>,
    mut definition_w: MessageWriter<LspDefinitionResponse>,
    mut references_w: MessageWriter<LspReferencesResponse>,
    mut format_w: MessageWriter<LspFormatResponse>,
) {
    for ev in drained.initialized.drain(..) {
        initialized_w.write(ev);
    }
    for ev in drained.diagnostics.drain(..) {
        diagnostics_w.write(ev);
    }
    for ev in drained.completion.drain(..) {
        completion_w.write(ev);
    }
    for ev in drained.resolved.drain(..) {
        resolved_w.write(ev);
    }
    for ev in drained.hover.drain(..) {
        hover_w.write(ev);
    }
    for ev in drained.definition.drain(..) {
        definition_w.write(ev);
    }
    for ev in drained.references.drain(..) {
        references_w.write(ev);
    }
    for ev in drained.format.drain(..) {
        format_w.write(ev);
    }
}

fn flush_drained_responses_b(
    mut drained: ResMut<DrainedResponses>,
    mut signature_w: MessageWriter<LspSignatureHelpResponse>,
    mut code_actions_w: MessageWriter<LspCodeActionsResponse>,
    mut inlay_w: MessageWriter<LspInlayHintsResponse>,
    mut highlights_w: MessageWriter<LspDocumentHighlightsResponse>,
    mut prepare_rename_w: MessageWriter<LspPrepareRenameResponse>,
    mut rename_w: MessageWriter<LspRenameResponse>,
    mut shutdown_w: MessageWriter<LspShutdownAck>,
    mut crashed_w: MessageWriter<LspServerCrashed>,
) {
    for ev in drained.signature.drain(..) {
        signature_w.write(ev);
    }
    for ev in drained.code_actions.drain(..) {
        code_actions_w.write(ev);
    }
    for ev in drained.inlay.drain(..) {
        inlay_w.write(ev);
    }
    for ev in drained.highlights.drain(..) {
        highlights_w.write(ev);
    }
    for ev in drained.prepare_rename.drain(..) {
        prepare_rename_w.write(ev);
    }
    for ev in drained.rename.drain(..) {
        rename_w.write(ev);
    }
    for ev in drained.shutdown.drain(..) {
        shutdown_w.write(ev);
    }
    for ev in drained.crashed.drain(..) {
        crashed_w.write(ev);
    }
}

/// Keeps the language server from being hard-killed mid-request.
fn shutdown_clients_on_app_exit(
    mut exit: MessageReader<AppExit>,
    clients: Query<&LspClient>,
) {
    if exit.read().next().is_none() {
        return;
    }
    for client in clients.iter() {
        client.shutdown();
    }
}
