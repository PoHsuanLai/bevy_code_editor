//! LSP (Language Server Protocol) core plugin
//!
//! This plugin provides LSP integration as an optional, decoupled component.
//! It listens to editor events and provides language server features like
//! completion, hover, diagnostics, etc.
//!
//! This plugin handles the LSP communication and state management only.
//! For UI rendering, use LspUiPlugin.

use bevy::prelude::*;

use crate::lsp::prelude::*;
use crate::lsp::state::{
    CodeActionState, CompletionState, DocumentHighlightState, HoverState, InlayHintState,
    LspSyncState, RenameState, SignatureHelpState,
};
use crate::lsp::systems::{
    cleanup_lsp_timeouts, process_lsp_messages, request_document_highlights, request_inlay_hints,
    sync_lsp_document, MultipleLocationsEvent, NavigateToFileEvent, WorkspaceEditEvent,
};
use crate::lsp::sync::{
    sync_code_actions_popup, sync_completion_popup, sync_document_highlights, sync_hover_popup,
    sync_inlay_hints, sync_rename_input, sync_signature_help_popup,
};
use crate::lsp::event_listeners::{
    listen_apply_completion, listen_completion_requests, listen_dismiss_completion,
    listen_hover_requests, listen_rename_requests, listen_signature_help_requests,
    listen_text_edit_events,
};
use crate::lsp::{LspUiRenderSet, LspUiSyncSet};

/// LSP core plugin providing language server integration
///
/// This plugin must be added AFTER CodeEditorPlugin.
/// It handles LSP communication and state management.
///
/// For UI rendering, add LspUiPlugin after this plugin.
/// For custom UI, query the marker components created by this plugin.
///
/// # Example
/// ```no_run
/// use bevy::prelude::*;
/// use bevy_code_editor::prelude::*;
///
/// App::new()
///     .add_plugins(CodeEditorPlugin::default())
///     .add_plugins(LspPlugin::default())
///     .add_plugins(LspUiPlugin::default())  // Optional: for default UI
///     .run();
/// ```
pub struct LspPlugin;

impl Default for LspPlugin {
    fn default() -> Self {
        Self
    }
}

impl LspPlugin {
    /// Create a new LSP plugin
    pub fn new() -> Self {
        Self::default()
    }
}

impl Plugin for LspPlugin {
    fn build(&self, app: &mut App) {
        // Core LSP resources
        app.insert_resource(LspClient::default());
        app.insert_resource(CompletionState::default());
        app.insert_resource(HoverState::default());
        app.insert_resource(LspSyncState::default());
        app.insert_resource(SignatureHelpState::default());
        app.insert_resource(CodeActionState::default());
        app.insert_resource(InlayHintState::default());
        app.insert_resource(DocumentHighlightState::default());
        app.insert_resource(RenameState::default());

        // Register LSP output events (LSP -> user code)
        app.add_message::<NavigateToFileEvent>();
        app.add_message::<MultipleLocationsEvent>();
        app.add_message::<WorkspaceEditEvent>();

        // Register LSP input events (editor -> LSP)
        app.add_message::<crate::events::RequestCompletionEvent>();
        app.add_message::<crate::events::RequestHoverEvent>();
        app.add_message::<crate::events::RequestRenameEvent>();
        app.add_message::<crate::events::RequestSignatureHelpEvent>();
        app.add_message::<crate::events::DismissCompletionEvent>();
        app.add_message::<crate::events::ApplyCompletionEvent>();

        // Configure system set ordering
        app.configure_sets(Update, LspUiSyncSet.before(LspUiRenderSet));

        // Core LSP systems (always enabled)
        app.add_systems(
            Update,
            (
                process_lsp_messages,
                sync_lsp_document,
                request_inlay_hints,
                request_document_highlights,
                cleanup_lsp_timeouts,
            ),
        );

        // LSP UI sync systems (state -> marker components)
        // These always run so users can query marker components
        app.add_systems(
            Update,
            (
                sync_completion_popup,
                sync_hover_popup,
                sync_signature_help_popup,
                sync_code_actions_popup,
                sync_rename_input,
                sync_inlay_hints,
                sync_document_highlights,
            )
                .in_set(LspUiSyncSet),
        );

        // Event listener systems (listen to editor events)
        app.add_systems(
            Update,
            (
                listen_text_edit_events,
                listen_completion_requests,
                listen_hover_requests,
                listen_rename_requests,
                listen_signature_help_requests,
                listen_dismiss_completion,
                listen_apply_completion,
            ),
        );
    }
}
