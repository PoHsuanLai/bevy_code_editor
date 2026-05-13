//! Editor-side LSP adapter plugin.
//!
//! Adds the LSP transport (`bevy_lsp::LspPlugin`) plus the editor's sync +
//! event-listener bridges. The plugin produces popup *data* (see
//! `lsp_ui::components`) and lets the host render however it wants — see
//! `examples/lsp.rs` for an `egui` + `armas` reference renderer.
//!
//! What this plugin *does* do:
//! - Register editor-only events (input requests like `CompletionRequested`,
//!   output events like `NavigateToFileEvent`, `MultipleLocationsEvent`,
//!   `WorkspaceEditEvent`).
//! - Drive `process_lsp_messages` (translates `LspResponse` → editor effects:
//!   move cursor, apply text edits, store hover content).
//! - Drive `sync_lsp_document` (debounced editor `did_change` notifications).
//! - Drive `request_inlay_hints` / `request_document_highlights` (cursor- and
//!   viewport-driven request fanout).
//! - Drive `tick_lsp_debounce_timers` (Zed-style tiered request debouncing).
//! - Drive sync systems that materialize state into marker components for the
//!   host renderer to query.
//! - Drive event-listener systems that translate editor events into
//!   `LspMessage::DidChange` / `LspMessage::Completion` / etc.

use bevy::prelude::*;
use bevy_instanced_text_edit::RopeBuffer;

use crate::lsp_ui::event_listeners::{
    advance_tabstop_session, dismiss_completion_on_cursor_move, drive_completion_resolve,
    end_tabstop_session_on_cursor_leave, listen_apply_completion, listen_completion_requests,
    listen_dismiss_completion, listen_hover_requests, listen_rename_requests,
    listen_signature_help_requests, listen_text_edit_events, tick_lsp_debounce_timers,
};
use crate::lsp_ui::state::LspCompletionPopup;
use crate::lsp_ui::sync::{
    sync_code_actions_popup, sync_completion_popup, sync_document_highlights, sync_hover_popup,
    sync_inlay_hints, sync_rename_input, sync_signature_help_popup,
};
use crate::lsp_ui::systems::{
    cleanup_lsp_timeouts, on_lsp_code_actions, on_lsp_completion, on_lsp_definition,
    on_lsp_diagnostics, on_lsp_document_highlights, on_lsp_format, on_lsp_hover,
    on_lsp_initialized, on_lsp_inlay_hints, on_lsp_prepare_rename, on_lsp_references,
    on_lsp_rename, on_lsp_resolved_completion, on_lsp_server_crashed, on_lsp_shutdown_ack,
    on_lsp_signature_help, request_document_highlights, request_inlay_hints, sync_lsp_document,
    MultipleLocationsEvent, NavigateToFileEvent, WorkspaceEditEvent,
};
use crate::settings::LspConfig;
use crate::types::CodeEditor;

/// LSP adapter plugin: bridges editor events to LSP requests, drains LSP
/// responses into editor state, and materializes state into marker components
/// for the host renderer.
///
/// Must be added **after** `CodeEditorPlugin`. The `bevy_lsp::LspPlugin`
/// transport is added idempotently. The host is responsible for rendering
/// popups — query the marker components materialized by this plugin's sync
/// systems; see `examples/lsp.rs` for an egui+armas reference.
///
/// # Example
/// ```no_run
/// use bevy::prelude::*;
/// use bevscode::prelude::*;
/// use bevy_lsp::LspPlugin as LspTransportPlugin;
///
/// App::new()
///     .add_plugins(CodeEditorPlugin)
///     .add_plugins(LspTransportPlugin)              // transport
///     .add_plugins(bevscode::plugin::LspPlugin)  // editor adapter
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
        // The transport layer lives in `bevy_lsp` (per-entity Components, not
        // Resources). `bevy_lsp::LspPlugin` is currently a no-op stable API
        // anchor; we still add it idempotently so future additions there
        // propagate to hosts that wire only this editor plugin.
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
        app.add_message::<crate::types::events::CompletionRequested>();
        app.add_message::<crate::types::events::HoverRequested>();
        app.add_message::<crate::types::events::RenameRequested>();
        app.add_message::<crate::types::events::SignatureHelpRequested>();
        app.add_message::<crate::types::events::CompletionDismissed>();
        app.add_message::<crate::types::events::CompletionApplied>();

        // Core LSP-driven systems. These query each editor entity for both
        // editor state (CursorState, TextBuffer<RopeBuffer>) and per-editor LSP
        // Components (LspClient, LspDocument, popup state, debounce timers).
        // The `on_lsp_*` systems each consume one outbound message from
        // `bevy_lsp` and apply it to per-editor Components. They run
        // in parallel where Bevy's scheduler can prove disjoint mutability.
        app.add_systems(
            Update,
            (
                on_lsp_initialized,
                on_lsp_diagnostics,
                on_lsp_completion,
                on_lsp_resolved_completion,
                on_lsp_hover,
                on_lsp_definition,
                on_lsp_references,
                on_lsp_format,
                on_lsp_signature_help,
                on_lsp_code_actions,
                on_lsp_inlay_hints,
            ),
        );
        app.add_systems(
            Update,
            (
                on_lsp_document_highlights,
                on_lsp_prepare_rename,
                on_lsp_rename,
                on_lsp_shutdown_ack,
                on_lsp_server_crashed,
                sync_lsp_document,
                request_inlay_hints,
                request_document_highlights,
                cleanup_lsp_timeouts,
                tick_lsp_debounce_timers,
            ),
        );

        // LSP UI sync systems (state -> marker components).
        // These always run so hosts can query marker components.
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
            ),
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
                dismiss_completion_on_cursor_move,
                drive_completion_resolve,
                sync_completion_settings,
                attach_snapshot_pre_edit_marker,
                end_tabstop_session_on_cursor_leave,
            ),
        );

        // Tabstop interception runs before the bevy_instanced_text_edit handler
        // for `InsertTabRequested` so the session consumes the event
        // when active. Schedule explicitly via system ordering.
        app.add_systems(
            Update,
            advance_tabstop_session
                .before(bevy_instanced_text_edit::handlers::edit::handle_insert_tab),
        );
    }
}

/// Mirror each editor's `LspConfig::completion::words_mode` onto its
/// `LspCompletionPopup` so the popup can gate filtering without re-reading settings.
fn sync_completion_settings(
    mut query: Query<(&LspConfig, &mut LspCompletionPopup), With<CodeEditor>>,
) {
    for (settings, mut popup) in &mut query {
        let target = settings.completion.words_mode;
        if popup.words_mode != target {
            popup.words_mode = target;
        }
    }
}

/// Attach [`bevy_instanced_text_edit::SnapshotPreEdit`] to any editor that has an
/// `LspDocument`. The marker tells `EditHistoryState::replace_range` to
/// snapshot the rope before mutating, so `listen_text_edit_events` can
/// build incremental `did_change` payloads with positions in the
/// negotiated wire encoding.
type AttachSnapshotQuery<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        With<CodeEditor>,
        With<bevy_lsp::LspDocument>,
        Without<bevy_instanced_text_edit::SnapshotPreEdit>,
    ),
>;

fn attach_snapshot_pre_edit_marker(mut commands: Commands, q: AttachSnapshotQuery) {
    for entity in q.iter() {
        commands
            .entity(entity)
            .insert(bevy_instanced_text_edit::SnapshotPreEdit);
    }
}
