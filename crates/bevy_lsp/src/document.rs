//! LSP document identifier — protocol-level state per open document.

use bevy::prelude::*;
use lsp_types::Url;

/// One open LSP document, keyed by URI per the protocol.
///
/// Pair with [`crate::LspClient`] on the same entity to define "the editor for
/// this URI." The `version` field is monotonically incremented on each
/// `did_change` send; consumers update it before calling
/// `LspClient::send(LspMessage::DidChange { ... })`.
#[derive(Component, Debug, Clone)]
pub struct LspDocument {
    pub uri: Url,
    pub version: i32,
    pub language_id: String,
}

impl LspDocument {
    /// Construct a new document with the given URI and language id, version 1.
    pub fn new(uri: Url, language_id: impl Into<String>) -> Self {
        Self {
            uri,
            version: 1,
            language_id: language_id.into(),
        }
    }

    /// Bump version and return the new value.
    pub fn bump_version(&mut self) -> i32 {
        self.version += 1;
        self.version
    }
}
