//! LSP (Language Server Protocol) integration
//!
//! This module provides LSP client functionality for advanced code editor features:
//! - Diagnostics (errors, warnings)
//! - Code completion
//! - Hover information
//! - Go to definition / Find references
//! - Code actions (quick fixes, refactoring)
//! - Signature help
//! - Inlay hints
//! - Document formatting
//!
//! ## Architecture
//!
//! The LSP module is organized into several submodules:
//!
//! - `client`: LSP client with JSON-RPC communication
//! - `messages`: Request/response message types
//! - `capabilities`: Server capability checking
//! - `state`: Bevy resources for UI state
//! - `components`: Marker components for UI elements
//! - `theme`: Theming configuration
//! - `sync`: Systems that sync state to marker entities
//! - `render`: Default render systems for UI elements
//! - `ui`: Legacy rendering systems (deprecated, use render)
//! - `systems`: Bevy systems for message processing
//!
//! ## Usage
//!
//! ```rust,ignore
//! use bevy_code_editor::lsp::prelude::*;
//!
//! // Create and start LSP client
//! let mut client = LspClient::new();
//! client.start("rust-analyzer", &[]).unwrap();
//!
//! // Send initialize request
//! client.send(LspMessage::Initialize {
//!     root_uri: Url::from_file_path("/my/project").unwrap(),
//!     capabilities: ClientCapabilities::default(),
//! });
//! ```
//!
//! ## Custom UI Rendering
//!
//! To replace the default UI with custom rendering:
//!
//! ```rust,ignore
//! use bevy_code_editor::prelude::*;
//! use bevy_code_editor::lsp::components::*;
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

pub mod capabilities;
pub mod client;
pub mod components;
pub mod event_listeners;
pub mod messages;
pub mod render;
pub mod state;
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
    pub use super::capabilities::ServerCapabilitiesCache;
    pub use super::client::{LspClient, DEFAULT_REQUEST_TIMEOUT_SECS};
    pub use super::components::{
        CodeActionItemData, CodeActionsPopupData, CompletionItemData, CompletionPopupData,
        DocumentHighlightData, HoverPopupData, InlayHintData, InlayHintKind, LspUiElement,
        LspUiVisual, RenameInputData, SignatureHelpPopupData,
    };
    pub use super::messages::{CodeActionOrCommand, LspMessage, LspResponse, RequestType};
    pub use super::state::{
        CodeActionState, CompletionState, HoverState, InlayHintState, LspSyncState,
        SignatureHelpState, UnifiedCompletionItem, WordCompletionItem,
        COMPLETION_MAX_VISIBLE_DEFAULT,
    };
    pub use super::sync::{
        sync_code_actions_popup, sync_completion_popup, sync_document_highlights,
        sync_hover_popup, sync_inlay_hints, sync_rename_input, sync_signature_help_popup,
    };
    pub use super::render::{
        cleanup_lsp_ui_visuals, render_code_actions_popup, render_completion_popup,
        render_document_highlights, render_hover_popup, render_inlay_hints,
        render_rename_input, render_signature_help_popup,
    };
    pub use super::systems::{
        cleanup_lsp_timeouts, execute_code_action, process_lsp_messages, request_code_actions,
        request_inlay_hints, request_signature_help, sync_lsp_document, DiagnosticMarker,
        LocationType, MultipleLocationsEvent, NavigateToFileEvent,
    };
    pub use super::event_listeners::{
        listen_apply_completion, listen_completion_requests, listen_dismiss_completion,
        listen_hover_requests, listen_rename_requests, listen_signature_help_requests,
        listen_text_edit_events,
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

// Re-export commonly used types at module level for backward compatibility
pub use client::LspClient;
pub use messages::{LspMessage, LspResponse};
pub use state::{CompletionState, HoverState, LspSyncState, UnifiedCompletionItem, WordCompletionItem, COMPLETION_MAX_VISIBLE_DEFAULT};
pub use systems::{
    process_lsp_messages, sync_lsp_document, DiagnosticMarker, LocationType,
    MultipleLocationsEvent, NavigateToFileEvent,
};
pub use ui::{update_completion_ui, update_hover_ui, CompletionUI, HoverUI};

/// Reset hover state helper (for backward compatibility)
pub fn reset_hover_state(hover_state: &mut HoverState) {
    hover_state.reset();
}
