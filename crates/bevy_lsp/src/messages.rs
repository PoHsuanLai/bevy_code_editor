//! LSP message types for communication with language servers

use lsp_types::*;
use serde::{Deserialize, Serialize};

/// Type of LSP request (used to match responses)
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

/// Messages sent to language server
#[derive(Debug, Clone)]
pub enum LspMessage {
    /// Initialize the language server
    Initialize {
        root_uri: Url,
        capabilities: ClientCapabilities,
    },

    /// Initialized notification
    Initialized,

    /// Text document opened
    DidOpen {
        uri: Url,
        language_id: String,
        version: i32,
        text: String,
    },

    /// Text document changed
    DidChange {
        uri: Url,
        version: i32,
        changes: Vec<TextDocumentContentChangeEvent>,
    },

    /// Request completion at position. `id` is opaque — `bevy_lsp` echoes
    /// it back on the matching `LspResponse::Completion` so the consumer
    /// can drop stale responses.
    Completion {
        uri: Url,
        position: Position,
        id: u64,
    },

    /// Resolve additional details for a completion item (docs, additional
    /// edits, etc.). Server fills in fields that were omitted from the
    /// initial completion response, gated on
    /// `completion_provider.resolve_provider`. `id` is opaque.
    ResolveCompletionItem { item: CompletionItem, id: u64 },

    /// Request hover information
    Hover { uri: Url, position: Position },

    /// Go to definition
    GotoDefinition { uri: Url, position: Position },

    /// Find references
    References { uri: Url, position: Position },

    /// Format document
    Format {
        uri: Url,
        options: FormattingOptions,
    },

    /// Request signature help
    SignatureHelp {
        uri: Url,
        position: Position,
        id: u64,
    },

    /// Request code actions
    CodeAction {
        uri: Url,
        range: Range,
        diagnostics: Vec<Diagnostic>,
        id: u64,
    },

    /// Request inlay hints
    InlayHint { uri: Url, range: Range },

    /// Execute a command (from code action)
    ExecuteCommand {
        command: String,
        arguments: Option<Vec<serde_json::Value>>,
    },

    /// Request document highlights (all occurrences of symbol under cursor)
    DocumentHighlight { uri: Url, position: Position },

    /// Prepare rename (check if rename is valid, get range)
    PrepareRename { uri: Url, position: Position },

    /// Perform rename
    Rename {
        uri: Url,
        position: Position,
        new_name: String,
    },

    /// Ask the server to shut down. Send before [`LspMessage::Exit`] for
    /// graceful termination. `id` is opaque; the response arrives as
    /// [`LspResponse::ShutdownAck`].
    Shutdown { id: u64 },

    /// Tell the server to exit. Send after `Shutdown` is acknowledged.
    /// Notification — no response.
    Exit,
}

/// Responses from language server
#[derive(Debug, Clone)]
pub enum LspResponse {
    /// Server initialized with capabilities
    Initialized { capabilities: ServerCapabilities },

    /// Diagnostics published
    Diagnostics {
        uri: Url,
        diagnostics: Vec<Diagnostic>,
    },

    /// Completion response. `id` echoes the request's id so the consumer
    /// can drop stale responses (cursor moved, more chars typed, etc.).
    Completion {
        id: u64,
        items: Vec<CompletionItem>,
        is_incomplete: bool,
    },

    /// Resolved completion item — same shape as the input but with
    /// `documentation`, `detail`, `additional_text_edits` filled in.
    /// `id` echoes the matching `ResolveCompletionItem` request.
    ResolvedCompletionItem { id: u64, item: CompletionItem },

    /// Hover response
    Hover {
        content: String,
        range: Option<Range>,
    },

    /// Definition location(s) - may have multiple definitions
    Definition { locations: Vec<Location> },

    /// Reference locations
    References { locations: Vec<Location> },

    /// Format edits
    Format { edits: Vec<TextEdit> },

    /// Signature help response
    SignatureHelp {
        id: u64,
        signatures: Vec<SignatureInformation>,
        active_signature: Option<u32>,
        active_parameter: Option<u32>,
    },

    /// Code actions response
    CodeActions {
        id: u64,
        actions: Vec<CodeActionOrCommand>,
    },

    /// Inlay hints response
    InlayHints { hints: Vec<InlayHint> },

    /// Document highlights response
    DocumentHighlights { highlights: Vec<DocumentHighlight> },

    /// Prepare rename response
    PrepareRename {
        range: Range,
        placeholder: Option<String>,
    },

    /// Rename response (workspace edit)
    Rename { edit: WorkspaceEdit },

    /// Acknowledgement of [`LspMessage::Shutdown`].
    ShutdownAck { id: u64 },

    /// Server crashed (process exited unexpectedly) or its read channel
    /// closed without an explicit shutdown. Hosts can react by either
    /// dropping the client or restarting it.
    Crashed,
}

/// Code action or command from LSP
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CodeActionOrCommand {
    Action(CodeAction),
    Command(Command),
}
