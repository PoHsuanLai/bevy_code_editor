//! `LspPlugin` — registers the LSP transport client and per-document sync state.
//!
//! This plugin is intentionally minimal: it inserts the [`LspClient`] resource
//! and the document-side state resources ([`LspSyncState`], [`LspDebounceTimers`],
//! [`CompletionState`], [`HoverState`], etc.) so any consumer (code editor,
//! debugger UI, AI panel, etc.) can compose its own systems on top.
//!
//! Driving the client (reading responses, dispatching by `RequestType`,
//! triggering `did_open` / `did_change` notifications) is the consumer's job.
//! See `crate::client::LspClient::try_recv` and `crate::client::LspClient::send`.

use bevy::prelude::*;

use crate::client::LspClient;
use crate::state::{
    CodeActionState, CompletionState, DocumentHighlightState, HoverState, InlayHintState,
    LspDebounceTimers, LspSyncState, RenameState, SignatureHelpState,
};

/// Plugin that registers the LSP transport client + per-document state
/// resources. Single concern, single plugin: no rendering, no editor coupling,
/// no auto-spawn of any servers.
///
/// Hosts that want a working "completion popup + hover popup" experience must
/// add their own systems that:
/// 1. Translate user input / cursor moves into [`LspMessage`](crate::messages::LspMessage) sends.
/// 2. Drain [`LspResponse`](crate::messages::LspResponse) via
///    [`LspClient::try_recv`](crate::client::LspClient::try_recv) and update the
///    state resources.
/// 3. Render the state into whatever UI they have.
///
/// `bevy_code_editor` ships an `lsp_ui` adapter that wires all of this up for
/// its editor entity; AI / debugger / code-search consumers will write their own
/// thinner adapters.
#[derive(Default)]
pub struct LspPlugin;

impl Plugin for LspPlugin {
    fn build(&self, app: &mut App) {
        // Reflect not registered: every resource in this crate embeds
        // `lsp_types::*` (CompletionItem, SignatureInformation, Url, Range,
        // DocumentHighlight, InlayHint, Position) or transport primitives
        // (`tokio::Runtime`, `mpsc::Sender`, child-process handles), none of
        // which implement `Reflect`. The LSP layer is also live-updated
        // off-thread so inspector access would be racy. Skipped intentionally.

        // Core transport.
        app.insert_resource(LspClient::default());

        // Per-document state, owned by the LSP layer because it's keyed by the
        // LSP protocol (Url + version + Position) rather than by any host UI.
        app.insert_resource(LspSyncState::default());
        app.insert_resource(LspDebounceTimers::default());

        // Feature-specific state resources. These are "the LSP's view of the
        // world" — what completions are pending, what hover content was last
        // returned. Hosts read them to drive their UI.
        app.insert_resource(CompletionState::default());
        app.insert_resource(HoverState::default());
        app.insert_resource(SignatureHelpState::default());
        app.insert_resource(CodeActionState::default());
        app.insert_resource(InlayHintState::default());
        app.insert_resource(DocumentHighlightState::default());
        app.insert_resource(RenameState::default());
    }
}
