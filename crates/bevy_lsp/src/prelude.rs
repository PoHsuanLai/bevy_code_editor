//! Convenient re-exports for typical consumer use.

pub use crate::capabilities::ServerCapabilities;
pub use crate::client::{LspClient, DEFAULT_REQUEST_TIMEOUT_SECS};
pub use crate::document::LspDocument;
pub use crate::messages::{CodeActionOrCommand, LspMessage, LspResponse, RequestType};
pub use crate::plugin::LspPlugin;
