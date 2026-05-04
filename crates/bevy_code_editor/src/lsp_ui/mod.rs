//! Editor-coupled LSP UI adapter
//!
//! The transport layer (JSON-RPC client, request/response routing, server
//! capability cache, document-sync state) lives in the peer crate
//! [`bevy_lsp`]. This module is the editor-coupled adapter: Sprite-based
//! popup rendering, theme, marker components, and the event-listener bridges
//! that translate editor `TextEditEvent`s, cursor moves, and key presses into
//! `LspMessage` sends. Hosts that prefer to drive popup rendering through
//! their own UI stack (e.g. egui+armas) can skip `LspUiPlugin` and read the
//! marker components in [`components`] directly — see `examples/lsp.rs`.
//!
//! Hosts that want completion / hover / etc. UI must add **both**:
//! - `bevy_lsp::LspPlugin` — the transport.
//! - `bevy_code_editor::plugin::LspPlugin` (this crate's editor-side plugin) —
//!   wires the UI sync, render, and event-listener systems.
//!
//! ## Custom UI Rendering
//!
//! To replace the default UI with custom rendering:
//!
//! ```rust,ignore
//! use bevy_code_editor::prelude::*;
//! use bevy_code_editor::lsp_ui::components::*;
//!
//! // Disable default UI when adding the plugin
//! app.add_plugins(
//!     CodeEditorPlugin::new()
//!         .with_lsp_ui(false)
//! );
//!
//! // Add your custom render system
//! app.add_systems(Update, my_custom_completion_renderer);
//!
//! fn my_custom_completion_renderer(
//!     query: Query<&CompletionPopupData, Changed<CompletionPopupData>>,
//!     mut commands: Commands,
//! ) {
//!     // Your custom rendering logic using the popup data
//! }
//! ```

use bevy::prelude::*;

pub mod components;
pub mod event_listeners;
pub mod render;
pub mod sync;
pub mod systems;
pub mod theme;
pub mod ui;

/// System set for LSP UI synchronization (state -> marker components)
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct LspUiSyncSet;

/// System set for LSP UI rendering (marker components -> visuals)
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct LspUiRenderSet;

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

    // Editor-coupled UI surface.
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
    pub use super::render::{
        cleanup_lsp_ui_visuals, render_code_actions_popup, render_completion_popup,
        render_document_highlights, render_hover_popup, render_inlay_hints, render_rename_input,
        render_signature_help_popup,
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
    pub use super::theme::{
        CodeActionsTheme, CommonTheme, CompletionTheme, DocumentHighlightsTheme, HoverTheme,
        InlayHintsTheme, LspUiTheme, RenameTheme, SignatureHelpTheme,
    };
    pub use super::ui::{
        update_code_action_ui, update_completion_ui, update_hover_ui, update_inlay_hints_ui,
        update_signature_help_ui, CodeActionUI, CompletionUI, HoverUI, InlayHintText,
        SignatureHelpUI,
    };
    pub use super::{LspUiRenderSet, LspUiSyncSet};
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
pub use ui::{update_completion_ui, update_hover_ui, CompletionUI, HoverUI};

/// Reset hover state helper (for backward compatibility)
pub fn reset_hover_state(hover_state: &mut bevy_lsp::HoverState) {
    hover_state.reset();
}
