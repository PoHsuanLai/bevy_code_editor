//! LSP message types for communication with language servers.

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
    Command(Command),
}
