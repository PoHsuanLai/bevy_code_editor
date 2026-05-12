//! Installs the Bevy message bus routes for LSP transport traffic.
//!
//! Every variant of [`LspResponse`] gets its own [`Message`] type so hosts
//! subscribe to just the shape they care about. The drain pipeline is split
//! into two stages so the writer fan-out can stay under Bevy's 16-parameter
//! system cap and so reader systems can run in parallel against the next
//! frame's drain.

use bevy::app::AppExit;
use bevy::prelude::*;

use crate::client::LspClient;
use crate::document::LspDocument;
use crate::messages::{LspRequest, *};

/// Registers all outbound LSP response messages, drives the drain pipeline
/// each frame, and gracefully shuts down every [`LspClient`] on `AppExit`.
///
/// Does **not** install a tokio runtime — the transport runs on Bevy's own
/// `AsyncComputeTaskPool` (smol-based).
#[derive(Default)]
pub struct LspPlugin;

impl Plugin for LspPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<LspServerInitialized>()
            .add_message::<LspDiagnosticsUpdated>()
            .add_message::<LspLogMessage>()
            .add_message::<LspShowMessage>()
            .add_message::<LspProgress>()
            .add_message::<LspTelemetry>()
            .add_message::<LspLogTrace>()
            .add_message::<LspConfigurationRequested>()
            .add_message::<LspApplyEditRequested>()
            .add_message::<LspShowMessageRequestRequested>()
            .add_message::<LspShowDocumentRequested>()
            .add_message::<LspWorkDoneProgressCreateRequested>()
            .add_message::<LspRegisterCapabilityRequested>()
            .add_message::<LspUnregisterCapabilityRequested>()
            .add_message::<LspWorkspaceFoldersRequested>()
            .add_message::<LspSemanticTokensRefreshRequested>()
            .add_message::<LspInlayHintRefreshRequested>()
            .add_message::<LspCodeLensRefreshRequested>()
            .add_message::<LspDiagnosticsRefreshRequested>()
            .add_message::<LspCompletionResponse>()
            .add_message::<LspResolvedCompletionItem>()
            .add_message::<LspHoverResponse>()
            .add_message::<LspSignatureHelpResponse>()
            .add_message::<LspDeclarationResponse>()
            .add_message::<LspDefinitionResponse>()
            .add_message::<LspTypeDefinitionResponse>()
            .add_message::<LspImplementationResponse>()
            .add_message::<LspReferencesResponse>()
            .add_message::<LspDocumentHighlightsResponse>()
            .add_message::<LspDocumentSymbolsResponse>()
            .add_message::<LspWorkspaceSymbolsResponse>()
            .add_message::<LspResolvedWorkspaceSymbol>()
            .add_message::<LspFoldingRangesResponse>()
            .add_message::<LspSelectionRangesResponse>()
            .add_message::<LspCodeActionsResponse>()
            .add_message::<LspResolvedCodeAction>()
            .add_message::<LspFormatResponse>()
            .add_message::<LspRangeFormattingResponse>()
            .add_message::<LspOnTypeFormattingResponse>()
            .add_message::<LspWillSaveWaitUntilResponse>()
            .add_message::<LspInlayHintsResponse>()
            .add_message::<LspResolvedInlayHint>()
            .add_message::<LspDocumentLinksResponse>()
            .add_message::<LspResolvedDocumentLink>()
            .add_message::<LspDocumentColorsResponse>()
            .add_message::<LspColorPresentationsResponse>()
            .add_message::<LspLinkedEditingRangesResponse>()
            .add_message::<LspMonikersResponse>()
            .add_message::<LspPrepareRenameResponse>()
            .add_message::<LspRenameResponse>()
            .add_message::<LspPrepareCallHierarchyResponse>()
            .add_message::<LspCallHierarchyIncomingCallsResponse>()
            .add_message::<LspCallHierarchyOutgoingCallsResponse>()
            .add_message::<LspPrepareTypeHierarchyResponse>()
            .add_message::<LspTypeHierarchySupertypesResponse>()
            .add_message::<LspTypeHierarchySubtypesResponse>()
            .add_message::<LspSemanticTokensResponse>()
            .add_message::<LspSemanticTokensDeltaResponse>()
            .add_message::<LspSemanticTokensRangeResponse>()
            .add_message::<LspDocumentDiagnosticResponse>()
            .add_message::<LspWorkspaceDiagnosticResponse>()
            .add_message::<LspShutdownAck>()
            .add_message::<LspServerCrashed>();

        app.add_message::<LspRequest>();
        app.add_observer(dispatch_lsp_request);

        app.init_resource::<DrainedResponses>();
        app.add_systems(
            Update,
            (
                flush_document_changes,
                drain_lsp_responses,
                flush_a.after(drain_lsp_responses),
                flush_b.after(drain_lsp_responses),
                flush_c.after(drain_lsp_responses),
                flush_d.after(drain_lsp_responses),
                flush_e.after(drain_lsp_responses),
            ),
        );
        app.add_systems(Last, shutdown_clients_on_app_exit);
    }
}

/// Buffered drain output. Splitting drain (mutable borrow of LspClient)
/// from writers (mutable borrow of message buses) keeps each system under
/// Bevy's 16-parameter cap and lets the scheduler parallelize reader
/// systems against the next drain.
#[derive(Resource, Default)]
struct DrainedResponses {
    initialized: Vec<LspServerInitialized>,
    diagnostics: Vec<LspDiagnosticsUpdated>,
    log_message: Vec<LspLogMessage>,
    show_message: Vec<LspShowMessage>,
    progress: Vec<LspProgress>,
    telemetry: Vec<LspTelemetry>,
    log_trace: Vec<LspLogTrace>,

    configuration_req: Vec<LspConfigurationRequested>,
    apply_edit_req: Vec<LspApplyEditRequested>,
    show_message_req: Vec<LspShowMessageRequestRequested>,
    show_document_req: Vec<LspShowDocumentRequested>,
    work_done_create_req: Vec<LspWorkDoneProgressCreateRequested>,
    register_cap_req: Vec<LspRegisterCapabilityRequested>,
    unregister_cap_req: Vec<LspUnregisterCapabilityRequested>,
    workspace_folders_req: Vec<LspWorkspaceFoldersRequested>,

    semantic_refresh_req: Vec<LspSemanticTokensRefreshRequested>,
    inlay_refresh_req: Vec<LspInlayHintRefreshRequested>,
    code_lens_refresh_req: Vec<LspCodeLensRefreshRequested>,
    diagnostics_refresh_req: Vec<LspDiagnosticsRefreshRequested>,

    completion: Vec<LspCompletionResponse>,
    resolved_completion: Vec<LspResolvedCompletionItem>,
    hover: Vec<LspHoverResponse>,
    signature_help: Vec<LspSignatureHelpResponse>,

    declaration: Vec<LspDeclarationResponse>,
    definition: Vec<LspDefinitionResponse>,
    type_definition: Vec<LspTypeDefinitionResponse>,
    implementation: Vec<LspImplementationResponse>,
    references: Vec<LspReferencesResponse>,
    highlights: Vec<LspDocumentHighlightsResponse>,
    document_symbols: Vec<LspDocumentSymbolsResponse>,
    workspace_symbols: Vec<LspWorkspaceSymbolsResponse>,
    resolved_workspace_symbol: Vec<LspResolvedWorkspaceSymbol>,
    folding: Vec<LspFoldingRangesResponse>,
    selection: Vec<LspSelectionRangesResponse>,

    code_actions: Vec<LspCodeActionsResponse>,
    resolved_code_action: Vec<LspResolvedCodeAction>,
    format: Vec<LspFormatResponse>,
    range_formatting: Vec<LspRangeFormattingResponse>,
    on_type_formatting: Vec<LspOnTypeFormattingResponse>,
    will_save_wait: Vec<LspWillSaveWaitUntilResponse>,

    inlay: Vec<LspInlayHintsResponse>,
    resolved_inlay: Vec<LspResolvedInlayHint>,
    doc_links: Vec<LspDocumentLinksResponse>,
    resolved_doc_link: Vec<LspResolvedDocumentLink>,
    doc_colors: Vec<LspDocumentColorsResponse>,
    color_presentations: Vec<LspColorPresentationsResponse>,
    linked_editing: Vec<LspLinkedEditingRangesResponse>,
    monikers: Vec<LspMonikersResponse>,

    prepare_rename: Vec<LspPrepareRenameResponse>,
    rename: Vec<LspRenameResponse>,

    prepare_call: Vec<LspPrepareCallHierarchyResponse>,
    incoming_calls: Vec<LspCallHierarchyIncomingCallsResponse>,
    outgoing_calls: Vec<LspCallHierarchyOutgoingCallsResponse>,
    prepare_type: Vec<LspPrepareTypeHierarchyResponse>,
    super_types: Vec<LspTypeHierarchySupertypesResponse>,
    sub_types: Vec<LspTypeHierarchySubtypesResponse>,

    semantic_tokens: Vec<LspSemanticTokensResponse>,
    semantic_delta: Vec<LspSemanticTokensDeltaResponse>,
    semantic_range: Vec<LspSemanticTokensRangeResponse>,
    doc_diagnostic: Vec<LspDocumentDiagnosticResponse>,
    ws_diagnostic: Vec<LspWorkspaceDiagnosticResponse>,

    shutdown_ack: Vec<LspShutdownAck>,
    crashed: Vec<LspServerCrashed>,
}

fn drain_lsp_responses(
    mut clients: Query<(Entity, &mut LspClient)>,
    mut d: ResMut<DrainedResponses>,
) {
    use crate::messages::LspResponse as R;
    for (entity, mut client) in clients.iter_mut() {
        client.cleanup_timeouts();
        while let Some(response) = client.try_recv() {
            match response {
                R::Initialized { capabilities } => {
                    client.initialized = true;
                    d.initialized.push(LspServerInitialized {
                        entity,
                        capabilities: *capabilities,
                    });
                }
                R::Diagnostics { uri, diagnostics } => {
                    d.diagnostics.push(LspDiagnosticsUpdated {
                        entity,
                        uri,
                        diagnostics,
                    });
                }
                R::LogMessage { typ, message } => {
                    d.log_message.push(LspLogMessage {
                        entity,
                        typ,
                        message,
                    });
                }
                R::ShowMessage { typ, message } => {
                    d.show_message.push(LspShowMessage {
                        entity,
                        typ,
                        message,
                    });
                }
                R::Progress { token, value } => {
                    d.progress.push(LspProgress {
                        entity,
                        token,
                        value,
                    });
                }
                R::Telemetry { data } => {
                    d.telemetry.push(LspTelemetry { entity, data });
                }
                R::LogTrace { message, verbose } => {
                    d.log_trace.push(LspLogTrace {
                        entity,
                        message,
                        verbose,
                    });
                }
                R::ConfigurationRequested { request_id, items } => {
                    d.configuration_req.push(LspConfigurationRequested {
                        entity,
                        request_id,
                        items,
                    });
                }
                R::ApplyEditRequested {
                    request_id,
                    label,
                    edit,
                } => {
                    d.apply_edit_req.push(LspApplyEditRequested {
                        entity,
                        request_id,
                        label,
                        edit,
                    });
                }
                R::ShowMessageRequestRequested {
                    request_id,
                    typ,
                    message,
                    actions,
                } => {
                    d.show_message_req.push(LspShowMessageRequestRequested {
                        entity,
                        request_id,
                        typ,
                        message,
                        actions,
                    });
                }
                R::ShowDocumentRequested {
                    request_id,
                    uri,
                    external,
                    take_focus,
                    selection,
                } => {
                    d.show_document_req.push(LspShowDocumentRequested {
                        entity,
                        request_id,
                        uri,
                        external,
                        take_focus,
                        selection,
                    });
                }
                R::WorkDoneProgressCreateRequested { request_id, token } => {
                    d.work_done_create_req
                        .push(LspWorkDoneProgressCreateRequested {
                            entity,
                            request_id,
                            token,
                        });
                }
                R::RegisterCapabilityRequested {
                    request_id,
                    registrations,
                } => {
                    d.register_cap_req.push(LspRegisterCapabilityRequested {
                        entity,
                        request_id,
                        registrations,
                    });
                }
                R::UnregisterCapabilityRequested {
                    request_id,
                    unregistrations,
                } => {
                    d.unregister_cap_req.push(LspUnregisterCapabilityRequested {
                        entity,
                        request_id,
                        unregistrations,
                    });
                }
                R::WorkspaceFoldersRequested { request_id } => {
                    d.workspace_folders_req
                        .push(LspWorkspaceFoldersRequested { entity, request_id });
                }
                R::SemanticTokensRefreshRequested => {
                    d.semantic_refresh_req
                        .push(LspSemanticTokensRefreshRequested { entity });
                }
                R::InlayHintRefreshRequested => {
                    d.inlay_refresh_req
                        .push(LspInlayHintRefreshRequested { entity });
                }
                R::CodeLensRefreshRequested => {
                    d.code_lens_refresh_req
                        .push(LspCodeLensRefreshRequested { entity });
                }
                R::DiagnosticsRefreshRequested => {
                    d.diagnostics_refresh_req
                        .push(LspDiagnosticsRefreshRequested { entity });
                }
                R::Completion {
                    id,
                    items,
                    is_incomplete,
                } => {
                    d.completion.push(LspCompletionResponse {
                        entity,
                        id,
                        items,
                        is_incomplete,
                    });
                }
                R::ResolvedCompletionItem { id, item } => {
                    d.resolved_completion.push(LspResolvedCompletionItem {
                        entity,
                        id,
                        item: *item,
                    });
                }
                R::Hover {
                    id,
                    content,
                    kind,
                    range,
                } => {
                    d.hover.push(LspHoverResponse {
                        entity,
                        id,
                        content,
                        kind,
                        range,
                    });
                }
                R::SignatureHelp {
                    id,
                    signatures,
                    active_signature,
                    active_parameter,
                } => {
                    d.signature_help.push(LspSignatureHelpResponse {
                        entity,
                        id,
                        signatures,
                        active_signature,
                        active_parameter,
                    });
                }
                R::Declaration { id, locations } => {
                    d.declaration.push(LspDeclarationResponse {
                        entity,
                        id,
                        locations,
                    });
                }
                R::Definition { id, locations } => {
                    d.definition.push(LspDefinitionResponse {
                        entity,
                        id,
                        locations,
                    });
                }
                R::TypeDefinition { id, locations } => {
                    d.type_definition.push(LspTypeDefinitionResponse {
                        entity,
                        id,
                        locations,
                    });
                }
                R::Implementation { id, locations } => {
                    d.implementation.push(LspImplementationResponse {
                        entity,
                        id,
                        locations,
                    });
                }
                R::References { id, locations } => {
                    d.references.push(LspReferencesResponse {
                        entity,
                        id,
                        locations,
                    });
                }
                R::DocumentHighlights { id, highlights } => {
                    d.highlights.push(LspDocumentHighlightsResponse {
                        entity,
                        id,
                        highlights,
                    });
                }
                R::DocumentSymbols { id, flat, nested } => {
                    d.document_symbols.push(LspDocumentSymbolsResponse {
                        entity,
                        id,
                        flat,
                        nested,
                    });
                }
                R::WorkspaceSymbols { id, symbols } => {
                    d.workspace_symbols.push(LspWorkspaceSymbolsResponse {
                        entity,
                        id,
                        symbols,
                    });
                }
                R::ResolvedWorkspaceSymbol { id, symbol } => {
                    d.resolved_workspace_symbol
                        .push(LspResolvedWorkspaceSymbol { entity, id, symbol });
                }
                R::FoldingRanges { id, ranges } => {
                    d.folding
                        .push(LspFoldingRangesResponse { entity, id, ranges });
                }
                R::SelectionRanges { id, ranges } => {
                    d.selection
                        .push(LspSelectionRangesResponse { entity, id, ranges });
                }
                R::CodeActions { id, actions } => {
                    d.code_actions.push(LspCodeActionsResponse {
                        entity,
                        id,
                        actions,
                    });
                }
                R::ResolvedCodeAction { id, action } => {
                    d.resolved_code_action.push(LspResolvedCodeAction {
                        entity,
                        id,
                        action: *action,
                    });
                }
                R::Format { id, edits } => {
                    d.format.push(LspFormatResponse { entity, id, edits });
                }
                R::RangeFormatting { id, edits } => {
                    d.range_formatting
                        .push(LspRangeFormattingResponse { entity, id, edits });
                }
                R::OnTypeFormatting { id, edits } => {
                    d.on_type_formatting
                        .push(LspOnTypeFormattingResponse { entity, id, edits });
                }
                R::WillSaveWaitUntil { id, edits } => {
                    d.will_save_wait
                        .push(LspWillSaveWaitUntilResponse { entity, id, edits });
                }
                R::InlayHints { id, hints } => {
                    d.inlay.push(LspInlayHintsResponse { entity, id, hints });
                }
                R::ResolvedInlayHint { id, hint } => {
                    d.resolved_inlay
                        .push(LspResolvedInlayHint { entity, id, hint });
                }
                R::DocumentLinks { id, links } => {
                    d.doc_links
                        .push(LspDocumentLinksResponse { entity, id, links });
                }
                R::ResolvedDocumentLink { id, link } => {
                    d.resolved_doc_link
                        .push(LspResolvedDocumentLink { entity, id, link });
                }
                R::DocumentColors { id, colors } => {
                    d.doc_colors
                        .push(LspDocumentColorsResponse { entity, id, colors });
                }
                R::ColorPresentations { id, presentations } => {
                    d.color_presentations.push(LspColorPresentationsResponse {
                        entity,
                        id,
                        presentations,
                    });
                }
                R::LinkedEditingRanges { id, ranges } => {
                    d.linked_editing
                        .push(LspLinkedEditingRangesResponse { entity, id, ranges });
                }
                R::Monikers { id, monikers } => {
                    d.monikers.push(LspMonikersResponse {
                        entity,
                        id,
                        monikers,
                    });
                }
                R::PrepareRename {
                    id,
                    range,
                    placeholder,
                } => {
                    d.prepare_rename.push(LspPrepareRenameResponse {
                        entity,
                        id,
                        range,
                        placeholder,
                    });
                }
                R::Rename { id, edit } => {
                    d.rename.push(LspRenameResponse { entity, id, edit });
                }
                R::PrepareCallHierarchy { id, items } => {
                    d.prepare_call
                        .push(LspPrepareCallHierarchyResponse { entity, id, items });
                }
                R::CallHierarchyIncomingCalls { id, calls } => {
                    d.incoming_calls
                        .push(LspCallHierarchyIncomingCallsResponse { entity, id, calls });
                }
                R::CallHierarchyOutgoingCalls { id, calls } => {
                    d.outgoing_calls
                        .push(LspCallHierarchyOutgoingCallsResponse { entity, id, calls });
                }
                R::PrepareTypeHierarchy { id, items } => {
                    d.prepare_type
                        .push(LspPrepareTypeHierarchyResponse { entity, id, items });
                }
                R::TypeHierarchySupertypes { id, items } => {
                    d.super_types
                        .push(LspTypeHierarchySupertypesResponse { entity, id, items });
                }
                R::TypeHierarchySubtypes { id, items } => {
                    d.sub_types
                        .push(LspTypeHierarchySubtypesResponse { entity, id, items });
                }
                R::SemanticTokens { id, result } => {
                    d.semantic_tokens
                        .push(LspSemanticTokensResponse { entity, id, result });
                }
                R::SemanticTokensDelta { id, result } => {
                    d.semantic_delta
                        .push(LspSemanticTokensDeltaResponse { entity, id, result });
                }
                R::SemanticTokensRange { id, result } => {
                    d.semantic_range
                        .push(LspSemanticTokensRangeResponse { entity, id, result });
                }
                R::DocumentDiagnostic { id, report } => {
                    d.doc_diagnostic
                        .push(LspDocumentDiagnosticResponse { entity, id, report });
                }
                R::WorkspaceDiagnostic { id, report } => {
                    d.ws_diagnostic
                        .push(LspWorkspaceDiagnosticResponse { entity, id, report });
                }
                R::ShutdownAck { id } => {
                    d.shutdown_ack.push(LspShutdownAck { entity, id });
                }
                R::Crashed => {
                    d.crashed.push(LspServerCrashed { entity });
                }
            }
        }
    }
}

macro_rules! flush {
    ($d:ident, $($field:ident => $writer:ident),* $(,)?) => {
        $(for ev in $d.$field.drain(..) { $writer.write(ev); })*
    };
}

#[allow(clippy::too_many_arguments)]
fn flush_a(
    mut d: ResMut<DrainedResponses>,
    mut initialized_w: MessageWriter<LspServerInitialized>,
    mut diagnostics_w: MessageWriter<LspDiagnosticsUpdated>,
    mut log_w: MessageWriter<LspLogMessage>,
    mut show_msg_w: MessageWriter<LspShowMessage>,
    mut progress_w: MessageWriter<LspProgress>,
    mut telemetry_w: MessageWriter<LspTelemetry>,
    mut log_trace_w: MessageWriter<LspLogTrace>,
    mut config_req_w: MessageWriter<LspConfigurationRequested>,
    mut apply_edit_w: MessageWriter<LspApplyEditRequested>,
    mut show_msg_req_w: MessageWriter<LspShowMessageRequestRequested>,
    mut show_doc_req_w: MessageWriter<LspShowDocumentRequested>,
    mut wd_create_w: MessageWriter<LspWorkDoneProgressCreateRequested>,
    mut reg_cap_w: MessageWriter<LspRegisterCapabilityRequested>,
    mut unreg_cap_w: MessageWriter<LspUnregisterCapabilityRequested>,
    mut ws_folders_w: MessageWriter<LspWorkspaceFoldersRequested>,
) {
    flush!(d,
        initialized => initialized_w,
        diagnostics => diagnostics_w,
        log_message => log_w,
        show_message => show_msg_w,
        progress => progress_w,
        telemetry => telemetry_w,
        log_trace => log_trace_w,
        configuration_req => config_req_w,
        apply_edit_req => apply_edit_w,
        show_message_req => show_msg_req_w,
        show_document_req => show_doc_req_w,
        work_done_create_req => wd_create_w,
        register_cap_req => reg_cap_w,
        unregister_cap_req => unreg_cap_w,
        workspace_folders_req => ws_folders_w,
    );
}

#[allow(clippy::too_many_arguments)]
fn flush_b(
    mut d: ResMut<DrainedResponses>,
    mut sem_refresh_w: MessageWriter<LspSemanticTokensRefreshRequested>,
    mut inlay_refresh_w: MessageWriter<LspInlayHintRefreshRequested>,
    mut codelens_refresh_w: MessageWriter<LspCodeLensRefreshRequested>,
    mut diag_refresh_w: MessageWriter<LspDiagnosticsRefreshRequested>,
    mut completion_w: MessageWriter<LspCompletionResponse>,
    mut resolved_w: MessageWriter<LspResolvedCompletionItem>,
    mut hover_w: MessageWriter<LspHoverResponse>,
    mut signature_w: MessageWriter<LspSignatureHelpResponse>,
    mut decl_w: MessageWriter<LspDeclarationResponse>,
    mut def_w: MessageWriter<LspDefinitionResponse>,
    mut type_def_w: MessageWriter<LspTypeDefinitionResponse>,
    mut impl_w: MessageWriter<LspImplementationResponse>,
    mut refs_w: MessageWriter<LspReferencesResponse>,
    mut highlights_w: MessageWriter<LspDocumentHighlightsResponse>,
) {
    flush!(d,
        semantic_refresh_req => sem_refresh_w,
        inlay_refresh_req => inlay_refresh_w,
        code_lens_refresh_req => codelens_refresh_w,
        diagnostics_refresh_req => diag_refresh_w,
        completion => completion_w,
        resolved_completion => resolved_w,
        hover => hover_w,
        signature_help => signature_w,
        declaration => decl_w,
        definition => def_w,
        type_definition => type_def_w,
        implementation => impl_w,
        references => refs_w,
        highlights => highlights_w,
    );
}

#[allow(clippy::too_many_arguments)]
fn flush_c(
    mut d: ResMut<DrainedResponses>,
    mut doc_sym_w: MessageWriter<LspDocumentSymbolsResponse>,
    mut ws_sym_w: MessageWriter<LspWorkspaceSymbolsResponse>,
    mut resolved_ws_w: MessageWriter<LspResolvedWorkspaceSymbol>,
    mut folding_w: MessageWriter<LspFoldingRangesResponse>,
    mut selection_w: MessageWriter<LspSelectionRangesResponse>,
    mut code_actions_w: MessageWriter<LspCodeActionsResponse>,
    mut resolved_ca_w: MessageWriter<LspResolvedCodeAction>,
    mut format_w: MessageWriter<LspFormatResponse>,
    mut range_fmt_w: MessageWriter<LspRangeFormattingResponse>,
    mut on_type_fmt_w: MessageWriter<LspOnTypeFormattingResponse>,
    mut will_save_w: MessageWriter<LspWillSaveWaitUntilResponse>,
    mut inlay_w: MessageWriter<LspInlayHintsResponse>,
    mut resolved_inlay_w: MessageWriter<LspResolvedInlayHint>,
    mut doc_links_w: MessageWriter<LspDocumentLinksResponse>,
) {
    flush!(d,
        document_symbols => doc_sym_w,
        workspace_symbols => ws_sym_w,
        resolved_workspace_symbol => resolved_ws_w,
        folding => folding_w,
        selection => selection_w,
        code_actions => code_actions_w,
        resolved_code_action => resolved_ca_w,
        format => format_w,
        range_formatting => range_fmt_w,
        on_type_formatting => on_type_fmt_w,
        will_save_wait => will_save_w,
        inlay => inlay_w,
        resolved_inlay => resolved_inlay_w,
        doc_links => doc_links_w,
    );
}

#[allow(clippy::too_many_arguments)]
fn flush_d(
    mut d: ResMut<DrainedResponses>,
    mut resolved_link_w: MessageWriter<LspResolvedDocumentLink>,
    mut doc_colors_w: MessageWriter<LspDocumentColorsResponse>,
    mut color_pres_w: MessageWriter<LspColorPresentationsResponse>,
    mut linked_edit_w: MessageWriter<LspLinkedEditingRangesResponse>,
    mut moniker_w: MessageWriter<LspMonikersResponse>,
    mut prep_rename_w: MessageWriter<LspPrepareRenameResponse>,
    mut rename_w: MessageWriter<LspRenameResponse>,
    mut prep_call_w: MessageWriter<LspPrepareCallHierarchyResponse>,
    mut incoming_w: MessageWriter<LspCallHierarchyIncomingCallsResponse>,
    mut outgoing_w: MessageWriter<LspCallHierarchyOutgoingCallsResponse>,
    mut prep_type_w: MessageWriter<LspPrepareTypeHierarchyResponse>,
    mut super_w: MessageWriter<LspTypeHierarchySupertypesResponse>,
    mut sub_w: MessageWriter<LspTypeHierarchySubtypesResponse>,
) {
    flush!(d,
        resolved_doc_link => resolved_link_w,
        doc_colors => doc_colors_w,
        color_presentations => color_pres_w,
        linked_editing => linked_edit_w,
        monikers => moniker_w,
        prepare_rename => prep_rename_w,
        rename => rename_w,
        prepare_call => prep_call_w,
        incoming_calls => incoming_w,
        outgoing_calls => outgoing_w,
        prepare_type => prep_type_w,
        super_types => super_w,
        sub_types => sub_w,
    );
}

#[allow(clippy::too_many_arguments)]
fn flush_e(
    mut d: ResMut<DrainedResponses>,
    mut sem_full_w: MessageWriter<LspSemanticTokensResponse>,
    mut sem_delta_w: MessageWriter<LspSemanticTokensDeltaResponse>,
    mut sem_range_w: MessageWriter<LspSemanticTokensRangeResponse>,
    mut doc_diag_w: MessageWriter<LspDocumentDiagnosticResponse>,
    mut ws_diag_w: MessageWriter<LspWorkspaceDiagnosticResponse>,
    mut shutdown_w: MessageWriter<LspShutdownAck>,
    mut crashed_w: MessageWriter<LspServerCrashed>,
) {
    flush!(d,
        semantic_tokens => sem_full_w,
        semantic_delta => sem_delta_w,
        semantic_range => sem_range_w,
        doc_diagnostic => doc_diag_w,
        ws_diagnostic => ws_diag_w,
        shutdown_ack => shutdown_w,
        crashed => crashed_w,
    );
}

fn flush_document_changes(
    mut query: Query<(Entity, &mut LspDocument), Changed<LspDocument>>,
    mut lsp_w: MessageWriter<LspRequest>,
) {
    for (entity, mut doc) in &mut query {
        let Some((version, changes)) = doc.take_changes() else {
            continue;
        };
        lsp_w.write(LspRequest {
            entity,
            msg: LspMessage::DidChange {
                uri: doc.uri.clone(),
                version,
                changes,
            },
        });
    }
}

fn dispatch_lsp_request(trigger: On<LspRequest>, clients: Query<&LspClient>) {
    let Ok(client) = clients.get(trigger.entity) else {
        return;
    };
    client.send(trigger.event().msg.clone());
}

fn shutdown_clients_on_app_exit(mut exit: MessageReader<AppExit>, clients: Query<&LspClient>) {
    if exit.read().next().is_none() {
        return;
    }
    for client in clients.iter() {
        client.shutdown();
    }
}
