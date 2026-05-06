//! LSP protocol layer for Bevy: JSON-RPC over stdio with per-entity Components
//! ([`LspClient`], [`LspDocument`], [`ServerCapabilities`]) so a host can run
//! many editors/servers at once. No popup state, fuzzy matching, or rendering —
//! UI lives in the host.

pub mod capabilities;
pub mod client;
pub mod document;
pub mod messages;
pub mod plugin;
pub mod pos;
pub mod prelude;

pub use crate::capabilities::ServerCapabilities;
pub use crate::client::{LspClient, DEFAULT_REQUEST_TIMEOUT_SECS};
pub use crate::document::LspDocument;
pub use crate::messages::{CodeActionOrCommand, LspMessage, LspResponse, RequestType};
pub use crate::plugin::LspPlugin;
pub use crate::pos::{
    lsp_position_to_rope_byte, lsp_position_to_rope_char, rope_byte_to_lsp_position,
    rope_char_to_lsp_position, rope_range_to_lsp_range, PositionEncoding,
};

pub use ::bevy_tokio_tasks;
pub use ::lsp_types;
