//! Editor-side LSP UI / adapter plugin.
//!
//! The transport (`LspClient`, JSON-RPC over stdio, request routing, document
//! sync state) lives in the peer crate `bevy_lsp`. **Hosts must add
//! `bevy_lsp::LspPlugin` themselves** alongside this plugin — we don't auto-add
//! it, mirroring the Phase-5 idiom used for `bevy_text_engine` and
//! `bevy_tree_sitter`. Doing so would make `bevy_lsp` effectively part of
//! `LspPlugin`'s surface, and hosts couldn't disable / replace it via standard
//! plugin tooling.
//!
//! What this plugin *does* do:
//! - Register editor-only events (input requests like `RequestCompletionEvent`,
//!   output events like `NavigateToFileEvent`, `MultipleLocationsEvent`,
//!   `WorkspaceEditEvent`).
//! - Drive `process_lsp_messages` (translates `LspResponse` → editor effects:
//!   move cursor, apply text edits, store hover content).
//! - Drive `sync_lsp_document` (debounced editor `did_change` notifications).
//! - Drive `request_inlay_hints` / `request_document_highlights` (cursor- and
//!   viewport-driven request fanout).
//! - Drive `tick_lsp_debounce_timers` (Zed-style tiered request debouncing).
//! - Drive sync systems that materialize state into marker components for the
//!   render systems (`LspUiPlugin`, or a host-supplied alternative) to query.
//! - Drive event-listener systems that translate editor events into
//!   `LspMessage::DidChange` / `LspMessage::Completion` / etc.

use bevy::prelude::*;

use crate::lsp_ui::event_listeners::{
    listen_apply_completion, listen_completion_requests, listen_dismiss_completion,
    listen_hover_requests, listen_rename_requests, listen_signature_help_requests,
    listen_text_edit_events, tick_lsp_debounce_timers,
};
use crate::lsp_ui::sync::{
    sync_code_actions_popup, sync_completion_popup, sync_document_highlights, sync_hover_popup,
    sync_inlay_hints, sync_rename_input, sync_signature_help_popup,
};
use crate::lsp_ui::systems::{
    cleanup_lsp_timeouts, process_lsp_messages, request_document_highlights, request_inlay_hints,
    sync_lsp_document, MultipleLocationsEvent, NavigateToFileEvent, WorkspaceEditEvent,
};
use crate::lsp_ui::{LspUiRenderSet, LspUiSyncSet};

/// LSP adapter plugin: bridges editor events to LSP requests, drains LSP
/// responses into editor state, and materializes state into marker components
/// for renderer plugins.
///
/// Must be added **after** `CodeEditorPlugin` and alongside `bevy_lsp::LspPlugin`.
///
/// For UI rendering, add `LspUiPlugin` after this plugin (sprite-based). For
/// fully custom UI (e.g. an egui+armas overlay; see `examples/lsp.rs`), skip
/// `LspUiPlugin` and query the marker components materialized by this
/// plugin's sync systems instead.
///
/// # Example
/// ```no_run
/// use bevy::prelude::*;
/// use bevy_code_editor::prelude::*;
/// use bevy_lsp::LspPlugin as LspTransportPlugin;
///
/// App::new()
///     .add_plugins(CodeEditorPlugin)
///     .add_plugins(LspTransportPlugin)              // transport
///     .add_plugins(bevy_code_editor::plugin::LspPlugin)  // editor adapter
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
        Self
    }
}

impl Plugin for LspPlugin {
    fn build(&self, app: &mut App) {
        // The transport layer (LspClient + state resources) lives in `bevy_lsp`.
        // Add it idempotently so that hosts that wire `CodeEditorPlugin` (which
        // auto-adds *this* plugin under the `lsp` feature) get a working
        // setup out of the box. Hosts that prefer to compose explicitly can
        // add `bevy_lsp::LspPlugin` themselves first; this no-ops in that case.
        if !app.is_plugin_added::<bevy_lsp::LspPlugin>() {
            app.add_plugins(bevy_lsp::LspPlugin);
        }

        // Editor-side output events (LSP responses → editor effects user code may observe).
        app.add_message::<NavigateToFileEvent>();
        app.add_message::<MultipleLocationsEvent>();
        app.add_message::<WorkspaceEditEvent>();

        // Editor-side input events (user keypress / mouse hover → LSP request).
        // These are intentionally *editor-side*: they're the host's vocabulary
        // for "I want a completion at this cursor position", which the listener
        // systems below translate into bevy_lsp::LspMessage variants.
        app.add_message::<crate::types::events::RequestCompletionEvent>();
        app.add_message::<crate::types::events::RequestHoverEvent>();
        app.add_message::<crate::types::events::RequestRenameEvent>();
        app.add_message::<crate::types::events::RequestSignatureHelpEvent>();
        app.add_message::<crate::types::events::DismissCompletionEvent>();
        app.add_message::<crate::types::events::ApplyCompletionEvent>();

        // Configure system set ordering
        app.configure_sets(Update, LspUiSyncSet.before(LspUiRenderSet));

        // Core LSP-driven systems. These need both the editor entity (to read
        // CursorState / TextViewState / apply edits) and the bevy_lsp resources
        // (LspClient, LspSyncState, etc.) — so they're editor-side adapters.
        app.add_systems(
            Update,
            (
                process_lsp_messages,
                sync_lsp_document,
                request_inlay_hints,
                request_document_highlights,
                cleanup_lsp_timeouts,
                tick_lsp_debounce_timers,
            ),
        );

        // LSP UI sync systems (state -> marker components).
        // These always run so users can query marker components.
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

        // Event listener systems (listen to editor events, fire LSP requests).
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
