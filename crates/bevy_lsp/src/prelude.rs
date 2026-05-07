//! Convenient re-exports for typical consumer use.
//!
//! This module re-exports the *whole* per-variant Bevy message surface so
//! a host can `use bevy_lsp::prelude::*;` and pick whichever
//! `MessageReader<Lsp*Response>` they care about.

pub use crate::capabilities::ServerCapabilities;
pub use crate::client::{LspClient, DEFAULT_REQUEST_TIMEOUT_SECS};
pub use crate::document::LspDocument;
pub use crate::messages::*;
pub use crate::plugin::LspPlugin;
pub use crate::pos::{
    lsp_position_to_rope_byte, lsp_position_to_rope_char, rope_byte_to_lsp_position,
    rope_char_to_lsp_position, rope_range_to_lsp_range, PositionEncoding,
};
