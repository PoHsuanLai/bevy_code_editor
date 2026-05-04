//! Data-only LSP UI adapter.
//!
//! The transport layer (JSON-RPC client, request/response routing, server
//! capability cache, document-sync state) lives in the peer crate
//! [`bevy_lsp`]. This module is the editor-coupled adapter that materializes
//! that transport state into per-popup data components and bridges editor
//! events to LSP requests. No rendering happens here — hosts read the marker
//! components from [`components`] and draw them however they prefer; see
//! `examples/lsp.rs` for an `egui` + `armas` reference renderer that also
//! handles inline decorations (inlay hints, document highlights) on the
//! sprite path.
//!
//! Hosts that want completion / hover / etc. data must add **both**:
//! - `bevy_lsp::LspPlugin` — the transport.
//! - `bevy_code_editor::plugin::LspPlugin` (this crate's editor-side plugin) —
//!   wires the sync + event-listener systems.

pub mod components;
pub mod event_listeners;
pub mod sync;
pub mod systems;

/// Prelude for convenient imports
pub mod prelude {
    // Transport-side surface — re-exported from bevy_lsp so consumers don't
    // have to know that's a separate crate at the import-line level.
    pub use bevy_lsp::{
        CodeActionOrCommand, CodeActionState, CompletionState, DocumentHighlightState, HoverState,
        InlayHintState, LspClient, LspDebounceTimers, LspMessage, LspResponse, LspSyncState,
        PendingCodeActionRequest, PendingLspRequest, RenameState, RequestType,
        ServerCapabilitiesCache, SignatureHelpState, UnifiedCompletionItem, WordCompletionItem,
        COMPLETION_MAX_VISIBLE_DEFAULT, DEFAULT_REQUEST_TIMEOUT_SECS,
    };

    // Editor-coupled UI surface (data-only).
    pub use super::components::{
        CodeActionItemData, CodeActionsPopupData, CompletionItemData, CompletionPopupData,
        DocumentHighlightData, HoverPopupData, InlayHintData, InlayHintKind, LspUiElement,
        LspUiVisual, RenameInputData, SignatureHelpPopupData,
    };
    pub use super::event_listeners::{
        listen_apply_completion, listen_completion_requests, listen_dismiss_completion,
        listen_hover_requests, listen_rename_requests, listen_signature_help_requests,
        listen_text_edit_events,
    };
    pub use super::sync::{
        sync_code_actions_popup, sync_completion_popup, sync_document_highlights, sync_hover_popup,
        sync_inlay_hints, sync_rename_input, sync_signature_help_popup,
    };
    pub use super::systems::{
        cleanup_lsp_timeouts, execute_code_action, process_lsp_messages, request_code_actions,
        request_inlay_hints, request_signature_help, sync_lsp_document, DiagnosticMarker,
        LocationType, MultipleLocationsEvent, NavigateToFileEvent,
    };
}

// Re-export commonly used types at module level for backward compatibility.
// Transport types come from bevy_lsp; UI types are local.
pub use bevy_lsp::{
    CodeActionOrCommand, CodeActionState, CompletionState, DocumentHighlightState, HoverState,
    InlayHintState, LspClient, LspDebounceTimers, LspMessage, LspResponse, LspSyncState,
    PendingCodeActionRequest, PendingLspRequest, RenameState, RequestType, ServerCapabilitiesCache,
    SignatureHelpState, UnifiedCompletionItem, WordCompletionItem, COMPLETION_MAX_VISIBLE_DEFAULT,
};
pub use systems::{
    process_lsp_messages, sync_lsp_document, DiagnosticMarker, LocationType,
    MultipleLocationsEvent, NavigateToFileEvent,
};

/// Reset hover state helper (for backward compatibility)
pub fn reset_hover_state(hover_state: &mut bevy_lsp::HoverState) {
    hover_state.reset();
}
