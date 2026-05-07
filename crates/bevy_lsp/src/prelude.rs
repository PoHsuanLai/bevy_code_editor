//! Convenient re-exports for typical consumer use.

pub use crate::capabilities::ServerCapabilities;
pub use crate::client::{LspClient, DEFAULT_REQUEST_TIMEOUT_SECS};
pub use crate::document::LspDocument;
pub use crate::messages::{
    CodeActionOrCommand, LspCodeActionsResponse, LspCompletionResponse, LspDefinitionResponse,
    LspDiagnosticsUpdated, LspDocumentHighlightsResponse, LspFormatResponse, LspHoverResponse,
    LspInlayHintsResponse, LspMessage, LspPrepareRenameResponse, LspReferencesResponse,
    LspRenameResponse, LspResolvedCompletionItem, LspResponse, LspServerCrashed,
    LspServerInitialized, LspShutdownAck, LspSignatureHelpResponse, RequestType,
};
pub use crate::plugin::LspPlugin;
