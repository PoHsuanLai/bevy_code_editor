//! Bevy systems for LSP integration.
//!
//! Drains [`bevy_lsp::LspResponse`]s into per-editor state Components, drives
//! debounced `did_change` notifications, and emits editor-side messages
//! (navigate, multiple-locations, workspace-edit) when the response flow
//! requires host action.

mod shared;

pub mod code_actions;
pub mod completion;
pub mod diagnostics;
pub mod dismiss;
pub mod document;
pub mod document_highlights;
pub mod formatting;
pub mod hover;
pub mod inlay_hints;
pub mod navigation;
pub mod rename;
pub mod server;
pub mod signature_help;

pub use code_actions::*;
pub use completion::*;
pub use diagnostics::*;
pub use dismiss::*;
pub use document::*;
pub use document_highlights::*;
pub use formatting::*;
pub use hover::*;
pub use inlay_hints::*;
pub use navigation::*;
pub use rename::*;
pub use server::*;
pub use signature_help::*;
