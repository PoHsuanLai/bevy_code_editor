//! LSP message types for communication with language servers

use lsp_types::*;
use serde::{Deserialize, Serialize};

/// Type of LSP request (used to match responses)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequestType {
    Initialize,
    Completion,
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

    /// Request completion at position
    Completion {
        uri: Url,
        position: Position,
    },

    /// Request hover information
    Hover {
        uri: Url,
        position: Position,
    },

    /// Go to definition
    GotoDefinition {
        uri: Url,
        position: Position,
    },

    /// Find references
    References {
        uri: Url,
        position: Position,
    },

    /// Format document
    Format {
        uri: Url,
        options: FormattingOptions,
    },

    /// Request signature help
    SignatureHelp {
        uri: Url,
        position: Position,
    },

    /// Request code actions
    CodeAction {
        uri: Url,
        range: Range,
        diagnostics: Vec<Diagnostic>,
    },

    /// Request inlay hints
    InlayHint {
        uri: Url,
        range: Range,
    },

    /// Execute a command (from code action)
    ExecuteCommand {
        command: String,
        arguments: Option<Vec<serde_json::Value>>,
    },

    /// Request document highlights (all occurrences of symbol under cursor)
    DocumentHighlight {
        uri: Url,
        position: Position,
    },

    /// Prepare rename (check if rename is valid, get range)
    PrepareRename {
        uri: Url,
        position: Position,
    },

    /// Perform rename
    Rename {
        uri: Url,
        position: Position,
        new_name: String,
    },
}

/// Responses from language server
#[derive(Debug, Clone)]
pub enum LspResponse {
    /// Server initialized with capabilities
    Initialized {
        capabilities: ServerCapabilities,
    },

    /// Diagnostics published
    Diagnostics {
        uri: Url,
        diagnostics: Vec<Diagnostic>,
    },

    /// Completion response
    Completion {
        items: Vec<CompletionItem>,
        is_incomplete: bool,
    },

    /// Hover response
    Hover {
        content: String,
        range: Option<Range>,
    },

    /// Definition location(s) - may have multiple definitions
    Definition {
        locations: Vec<Location>,
    },

    /// Reference locations
    References {
        locations: Vec<Location>,
    },

    /// Format edits
    Format {
        edits: Vec<TextEdit>,
    },

    /// Signature help response
    SignatureHelp {
        signatures: Vec<SignatureInformation>,
        active_signature: Option<u32>,
        active_parameter: Option<u32>,
    },

    /// Code actions response
    CodeActions {
        actions: Vec<CodeActionOrCommand>,
    },

    /// Inlay hints response
    InlayHints {
        hints: Vec<InlayHint>,
    },

    /// Document highlights response
    DocumentHighlights {
        highlights: Vec<DocumentHighlight>,
    },

    /// Prepare rename response
    PrepareRename {
        range: Range,
        placeholder: Option<String>,
    },

    /// Rename response (workspace edit)
    Rename {
        edit: WorkspaceEdit,
    },
}

/// Code action or command from LSP
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CodeActionOrCommand {
    Action(CodeAction),
    Command(Command),
}
