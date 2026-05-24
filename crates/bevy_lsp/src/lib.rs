#![allow(clippy::type_complexity)]

//! Language Server Protocol transport for Bevy.
//!
//! Manages a JSON-RPC stdio connection to a language server as ECS components.
//! This crate is UI-agnostic — it speaks the wire protocol and delivers
//! typed responses as Bevy messages. What you do with those responses
//! (completion popups, hover tooltips, inline diagnostics, outline panels)
//! is entirely up to the host.
//!
//! ## Concepts
//!
//! Attach these components to an entity (one per language server session):
//!
//! - **[`LspClient`]** — the live connection to the server. Call
//!   [`LspClient::send`] with an [`LspMessage`] to issue a request or
//!   notification; responses arrive as Bevy messages on the same entity.
//! - **[`LspDocument`]** — tracks the URI and version for one open file.
//!   Required on entities that need per-document requests (completion, hover,
//!   diagnostics). Not cascaded automatically — the host supplies the URI.
//! - **[`ServerCapabilities`]** — server feature flags negotiated during
//!   `initialize`. Query this before sending optional requests.
//!
//! ## Dispatch model
//!
//! [`LspPlugin`] drains the async transport each frame and fans server
//! responses out as typed `Lsp*Response` Bevy messages tagged with the
//! originating entity. Subscribe with `MessageReader<LspCompletionResponse>`,
//! `MessageReader<LspDiagnosticsResponse>`, etc. to react without owning the
//! client directly. Coverage is the full LSP 3.17 spec.
//!
//! ## Rope ↔ LSP position helpers
//!
//! The [`pos`] module converts between `ropey` rope offsets and LSP
//! `Position` / byte offsets in UTF-8, UTF-16, or UTF-32 encodings, matching
//! whatever `positionEncoding` the server negotiated.
//!
//! ## What this crate does NOT provide
//!
//! - Completion popup state or fuzzy matching
//! - Hover tooltip rendering
//! - Inline diagnostic squiggles
//! - Any UI whatsoever — all of that lives in the host
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use bevy_app::{App, AppExit};
//! use bevy_lsp::LspPlugin;
//!
//! let _: AppExit = App::new().add_plugins(LspPlugin).run();
//! ```

pub mod capabilities;
pub mod client;
mod dispatch;
pub mod document;
pub mod messages;
pub mod plugin;
pub mod pos;
pub mod prelude;
pub mod transport;

#[cfg(all(not(target_arch = "wasm32"), any(test, feature = "test-support")))]
pub mod test_support;

pub use crate::capabilities::ServerCapabilities;
pub use crate::client::{LspClient, DEFAULT_REQUEST_TIMEOUT_SECS};
pub use crate::document::LspDocument;
pub use crate::messages::{LspRequest, *};
pub use crate::plugin::LspPlugin;
pub use crate::pos::{
    lsp_position_to_rope_byte, lsp_position_to_rope_char, rope_byte_to_lsp_position,
    rope_char_to_lsp_position, rope_range_to_lsp_range, PositionEncoding,
};
#[cfg(not(target_arch = "wasm32"))]
pub use crate::transport::StdioTransport;
#[cfg(target_arch = "wasm32")]
pub use crate::transport::WebSocketTransport;
pub use crate::transport::{LspTransport, TransportHandle};

pub use ::lsp_types;
