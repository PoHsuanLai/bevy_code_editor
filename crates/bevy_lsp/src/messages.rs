//! LSP message types for communication with language servers.
//!
//! Two layers:
//! - [`LspMessage`] / [`LspResponse`] are the protocol DTOs that flow over the
//!   async transport channel inside `LspClient`. Internal to the crate's
//!   transport.
//! - The `LspResponse*` `Message` types below mirror each [`LspResponse`]
//!   variant onto Bevy's message bus, tagged with the originating
//!   [`Entity`]. `LspPlugin` runs `drain_lsp_responses` each frame to fan the
//!   transport channel out into typed Bevy messages so any host system can
//!   subscribe without owning the [`LspClient`].

use bevy::prelude::*;
use lsp_types::*;
use serde::{Deserialize, Serialize};

/// Type of LSP request, used to match responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequestType {
    Initialize,
    Completion,
    CompletionItemResolve,
    Hover,
    GotoDefinition,
    References,
    Format,
    SignatureHelp,
    CodeAction,
    InlayHint,
    DocumentHighlight,
    PrepareRename,
    Rename,
    Shutdown,
}

/// Outgoing message to the language server.
#[derive(Debug, Clone)]
pub enum LspMessage {
    Initialize {
        root_uri: Url,
        capabilities: ClientCapabilities,
    },

    Initialized,

    DidOpen {
        uri: Url,
        language_id: String,
        version: i32,
        text: String,
    },

    DidChange {
        uri: Url,
        version: i32,
        changes: Vec<TextDocumentContentChangeEvent>,
    },

    /// `id` is opaque — echoed back on the matching [`LspResponse::Completion`]
    /// so consumers can drop stale responses.
    Completion {
        uri: Url,
        position: Position,
        id: u64,
    },

    /// Lazy-load completion details (docs, additional edits). Gated on
    /// `completion_provider.resolve_provider`. `id` is opaque.
    ResolveCompletionItem { item: CompletionItem, id: u64 },

    Hover { uri: Url, position: Position },

    GotoDefinition { uri: Url, position: Position },

    References { uri: Url, position: Position },

    Format {
        uri: Url,
        options: FormattingOptions,
    },

    SignatureHelp {
        uri: Url,
        position: Position,
        id: u64,
    },

    CodeAction {
        uri: Url,
        range: Range,
        diagnostics: Vec<Diagnostic>,
        id: u64,
    },

    InlayHint { uri: Url, range: Range },

    /// Execute a command produced by a code action.
    ExecuteCommand {
        command: String,
        arguments: Option<Vec<serde_json::Value>>,
    },

    /// All occurrences of the symbol under the cursor.
    DocumentHighlight { uri: Url, position: Position },

    /// Check whether rename is valid at `position`, and get the range.
    PrepareRename { uri: Url, position: Position },

    Rename {
        uri: Url,
        position: Position,
        new_name: String,
    },

    /// Send before [`LspMessage::Exit`] for graceful termination. `id` is
    /// opaque; the response arrives as [`LspResponse::ShutdownAck`].
    Shutdown { id: u64 },

    /// Tell the server to exit. Notification — no response.
    Exit,
}

/// Incoming response from the language server.
#[derive(Debug, Clone)]
pub enum LspResponse {
    Initialized { capabilities: ServerCapabilities },

    Diagnostics {
        uri: Url,
        diagnostics: Vec<Diagnostic>,
    },

    /// `id` echoes the request id so consumers can drop stale responses.
    Completion {
        id: u64,
        items: Vec<CompletionItem>,
        is_incomplete: bool,
    },

    /// Same shape as the input, with `documentation`, `detail`, and
    /// `additional_text_edits` filled in. `id` echoes the request.
    ResolvedCompletionItem { id: u64, item: CompletionItem },

    /// `kind` reports the source format the server returned (most send
    /// `Markdown`; some send `PlainText`). UI consumers route accordingly.
    Hover {
        content: String,
        kind: MarkupKind,
        range: Option<Range>,
    },

    Definition { locations: Vec<Location> },

    References { locations: Vec<Location> },

    Format { edits: Vec<TextEdit> },

    SignatureHelp {
        id: u64,
        signatures: Vec<SignatureInformation>,
        active_signature: Option<u32>,
        active_parameter: Option<u32>,
    },

    CodeActions {
        id: u64,
        actions: Vec<CodeActionOrCommand>,
    },

    InlayHints { hints: Vec<InlayHint> },

    DocumentHighlights { highlights: Vec<DocumentHighlight> },

    PrepareRename {
        range: Range,
        placeholder: Option<String>,
    },

    Rename { edit: WorkspaceEdit },

    ShutdownAck { id: u64 },

    /// Server process exited unexpectedly or its read channel closed
    /// without an explicit shutdown. Hosts can drop or restart the client.
    Crashed,
}

/// Code action or command returned by the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CodeActionOrCommand {
    Action(CodeAction),
    Command(lsp_types::Command),
}

// ─── Bevy messages ─────────────────────────────────────────────────────
//
// One-to-one with [`LspResponse`] variants so hosts can subscribe to just the
// shape they care about. The drain system in [`crate::plugin`] writes each
// of these in response to messages arriving on the transport channel. None
// of them are `Reflect` — the lsp_types payloads aren't.

/// Server completed `initialize` and reported its capabilities. Hosts can
/// subscribe to enable UI affordances that depend on capability bits
/// (`hover_provider`, `completion_provider`, etc.). Mirrored onto
/// `LspClient.initialized = true` by the drain system.
#[derive(Message, Clone, Debug)]
pub struct LspServerInitialized {
    pub entity: Entity,
    pub capabilities: ServerCapabilities,
}

/// `textDocument/publishDiagnostics` notification arrived. Hosts that render
/// diagnostics (gutter markers, hover squiggles, problem panels) subscribe.
#[derive(Message, Clone, Debug)]
pub struct LspDiagnosticsUpdated {
    pub entity: Entity,
    pub uri: Url,
    pub diagnostics: Vec<Diagnostic>,
}

/// `textDocument/completion` response. `id` echoes the request so consumers
/// can drop stale responses after the user kept typing.
#[derive(Message, Clone, Debug)]
pub struct LspCompletionResponse {
    pub entity: Entity,
    pub id: u64,
    pub items: Vec<CompletionItem>,
    pub is_incomplete: bool,
}

/// `completionItem/resolve` filled in detail/documentation/additionalTextEdits.
#[derive(Message, Clone, Debug)]
pub struct LspResolvedCompletionItem {
    pub entity: Entity,
    pub id: u64,
    pub item: CompletionItem,
}

/// `textDocument/hover` response.
#[derive(Message, Clone, Debug)]
pub struct LspHoverResponse {
    pub entity: Entity,
    pub content: String,
    pub kind: MarkupKind,
    pub range: Option<Range>,
}

/// `textDocument/definition` response.
#[derive(Message, Clone, Debug)]
pub struct LspDefinitionResponse {
    pub entity: Entity,
    pub locations: Vec<Location>,
}

/// `textDocument/references` response.
#[derive(Message, Clone, Debug)]
pub struct LspReferencesResponse {
    pub entity: Entity,
    pub locations: Vec<Location>,
}

/// `textDocument/formatting` response.
#[derive(Message, Clone, Debug)]
pub struct LspFormatResponse {
    pub entity: Entity,
    pub edits: Vec<TextEdit>,
}

/// `textDocument/signatureHelp` response.
#[derive(Message, Clone, Debug)]
pub struct LspSignatureHelpResponse {
    pub entity: Entity,
    pub id: u64,
    pub signatures: Vec<SignatureInformation>,
    pub active_signature: Option<u32>,
    pub active_parameter: Option<u32>,
}

/// `textDocument/codeAction` response.
#[derive(Message, Clone, Debug)]
pub struct LspCodeActionsResponse {
    pub entity: Entity,
    pub id: u64,
    pub actions: Vec<CodeActionOrCommand>,
}

/// `textDocument/inlayHint` response.
#[derive(Message, Clone, Debug)]
pub struct LspInlayHintsResponse {
    pub entity: Entity,
    pub hints: Vec<InlayHint>,
}

/// `textDocument/documentHighlight` response.
#[derive(Message, Clone, Debug)]
pub struct LspDocumentHighlightsResponse {
    pub entity: Entity,
    pub highlights: Vec<DocumentHighlight>,
}

/// `textDocument/prepareRename` response.
#[derive(Message, Clone, Debug)]
pub struct LspPrepareRenameResponse {
    pub entity: Entity,
    pub range: Range,
    pub placeholder: Option<String>,
}

/// `textDocument/rename` response (workspace edit).
#[derive(Message, Clone, Debug)]
pub struct LspRenameResponse {
    pub entity: Entity,
    pub edit: WorkspaceEdit,
}

/// Server acknowledged `shutdown`. Caller follows up with `Exit`.
#[derive(Message, Clone, Debug)]
pub struct LspShutdownAck {
    pub entity: Entity,
    pub id: u64,
}

/// Server crashed or its read channel closed without a graceful shutdown.
/// Hosts should clear popup state and either drop or restart the client.
#[derive(Message, Clone, Debug)]
pub struct LspServerCrashed {
    pub entity: Entity,
}
