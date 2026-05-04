//! `LspPlugin` — stable API anchor for the LSP transport layer.
//!
//! Currently a no-op: the protocol-level state ([`crate::LspClient`],
//! [`crate::LspDocument`], [`crate::ServerCapabilities`]) is per-entity and
//! inserted by the host (or via the editor's `#[require]` cascade), so no
//! global Resources need registering. The plugin is kept as a zero-overhead
//! anchor for future additions (server registry, multi-document indexing, etc.)
//! and as a stable composition point for hosts that want to "add the LSP
//! transport" symbolically.

use bevy::prelude::*;

/// LSP transport plugin.
///
/// Empty by design — see module docs.
#[derive(Default)]
pub struct LspPlugin;

impl Plugin for LspPlugin {
    fn build(&self, _app: &mut App) {
        // No-op: per-document state is Components, not Resources, so there is
        // nothing to register at the App level.
    }
}
