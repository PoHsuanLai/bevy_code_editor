//! # bevy_lsp
//!
//! Text-rendering-agnostic Language Server Protocol integration for Bevy.
//!
//! This crate provides the LSP transport layer + per-document state resources,
//! independent of any text-rendering or editor UI. Suitable for:
//!
//! - Code editors (see `bevy_code_editor`'s `lsp_ui` adapter for a worked
//!   example).
//! - Debugger UIs that want to query symbols / hover info from a running LSP.
//! - Hot-reload tooling that watches diagnostics on save.
//! - AI chat panels that consult an LSP for completions / type info.
//! - Code-search tools that consume `workspace/symbol` results.
//!
//! ## What's in the box
//!
//! - [`LspClient`] — JSON-RPC over stdio with channel-based async I/O,
//!   reader / writer threads, capability-aware send-gating.
//! - [`LspMessage`] / [`LspResponse`] / [`RequestType`] — typed wrappers over
//!   the LSP wire protocol.
//! - [`ServerCapabilitiesCache`] — query helpers for the
//!   `serverInfo.capabilities` returned at `initialize` time.
//! - State resources tracking per-document sync, completion, hover, signature
//!   help, code-action, document-highlight, rename, and inlay-hint state.
//! - [`LspPlugin`] — registers the resources above. Drives nothing; consumers
//!   add their own systems that translate input to `LspMessage` sends and
//!   drain `LspResponse`s into state.
//!
//! ## What's NOT in the box
//!
//! - No rendering: this crate has no `bevy_render` / `bevy_text_engine`
//!   dependency. Editor-coupled bits (popup rendering, theme, marker
//!   components) live in the editor crate.
//! - No event types tied to a specific text-buffer representation: hosts that
//!   want to bridge their own `TextEdit`-like events into LSP `did_change`
//!   notifications do so by calling
//!   [`LspClient::send`][LspClient#method.send] from their own adapter
//!   systems.
//! - No auto-add of unrelated plugins (tree-sitter, etc.). Single concern.
//!
//! ## Usage sketch
//!
//! ```no_run
//! use bevy::prelude::*;
//! use bevy_lsp::prelude::*;
//!
//! App::new()
//!     .add_plugins(MinimalPlugins)
//!     .add_plugins(LspPlugin)
//!     .add_systems(Startup, |mut client: ResMut<LspClient>| {
//!         client.start("rust-analyzer", &[]).unwrap();
//!     })
//!     .run();
//! ```

pub mod capabilities;
pub mod client;
pub mod messages;
pub mod plugin;
pub mod prelude;
pub mod state;

pub use crate::capabilities::ServerCapabilitiesCache;
pub use crate::client::{LspClient, DEFAULT_REQUEST_TIMEOUT_SECS};
pub use crate::messages::{CodeActionOrCommand, LspMessage, LspResponse, RequestType};
pub use crate::plugin::LspPlugin;
pub use crate::state::{
    CodeActionState, CompletionState, DocumentHighlightState, HoverState, InlayHintState,
    LspDebounceTimers, LspSyncState, PendingCodeActionRequest, PendingLspRequest, RenameState,
    SignatureHelpState, UnifiedCompletionItem, WordCompletionItem, COMPLETION_MAX_VISIBLE_DEFAULT,
};

// Re-export the underlying lsp-types crate so consumers can name
// `lsp_types::Url`, `lsp_types::Position`, etc. without taking a direct dep.
pub use ::lsp_types;
