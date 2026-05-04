//! # bevy_lsp
//!
//! What LSP does in Bevy, nothing more, nothing less.
//!
//! This crate is the protocol layer: JSON-RPC over stdio, request/response ID
//! routing, server-capability querying, per-document URI/version tracking. All
//! of it lives as **per-entity Components** rather than global Resources, so a
//! host application can have many editors / many servers at once just by
//! spawning entities.
//!
//! ## What's in the box
//!
//! - [`LspClient`] — Component owning the transport (writer/reader threads,
//!   pending-request map). One per server connection.
//! - [`LspDocument`] — Component identifying one open document on that server
//!   by URI + monotonically incrementing version + language id.
//! - [`ServerCapabilities`] — Component holding the parsed capabilities from
//!   the `initialize` response. Populated by the editor adapter (or any
//!   consumer) when it observes [`LspResponse::Initialized`].
//! - [`LspMessage`] / [`LspResponse`] / [`RequestType`] — typed wrappers over
//!   the LSP wire protocol.
//! - [`LspPlugin`] — empty plugin kept as a stable API anchor.
//!
//! ## What's NOT in the box
//!
//! - No popup state, no "what's currently selected", no fuzzy matching, no
//!   completion filtering / debouncing / word-fallback. Those are UI choices
//!   that belong to the editor (or whatever host owns a UI).
//! - No rendering: this crate has no `bevy_render` / `bevy_text_engine`
//!   dependency.
//! - No event types tied to a specific text-buffer representation: hosts that
//!   want to bridge their own `TextEdit`-like events into LSP `did_change`
//!   notifications do so by calling [`LspClient::send`] from their own
//!   adapter systems.
//!
//! ## Usage sketch
//!
//! ```no_run
//! use bevy::prelude::*;
//! use bevy_lsp::prelude::*;
//! use bevy_tokio_tasks::TokioTasksRuntime;
//!
//! fn setup(mut commands: Commands, runtime: ResMut<TokioTasksRuntime>) {
//!     let uri = lsp_types::Url::parse("file:///tmp/foo.rs").unwrap();
//!     let mut client = LspClient::new();
//!     client.start(&runtime, "rust-analyzer", &[]).unwrap();
//!     commands.spawn((
//!         client,
//!         LspDocument::new(uri, "rust"),
//!         ServerCapabilities::default(),
//!     ));
//! }
//! ```

pub mod capabilities;
pub mod client;
pub mod document;
pub mod messages;
pub mod plugin;
pub mod prelude;

pub use crate::capabilities::ServerCapabilities;
pub use crate::client::{LspClient, DEFAULT_REQUEST_TIMEOUT_SECS};
pub use crate::document::LspDocument;
pub use crate::messages::{CodeActionOrCommand, LspMessage, LspResponse, RequestType};
pub use crate::plugin::LspPlugin;

// Re-export the underlying lsp-types crate so consumers can name
// `lsp_types::Url`, `lsp_types::Position`, etc. without taking a direct dep.
pub use ::lsp_types;

// Re-export the tokio-tasks integration so callers of `LspClient::start` can
// name `TokioTasksRuntime` without taking a direct dep on the integration
// crate.
pub use ::bevy_tokio_tasks;
