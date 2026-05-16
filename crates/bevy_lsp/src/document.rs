//! LSP document identifier — protocol-level state per open document.

use bevy_ecs::prelude::*;
use lsp_types::{TextDocumentContentChangeEvent, Url};

/// One open LSP document, keyed by URI per the protocol.
///
/// Attach to the same entity as [`crate::LspClient`]. When the document
/// content changes, call [`LspDocument::push_change`] or
/// [`LspDocument::push_full_sync`] — [`crate::LspPlugin`] detects the
/// mutation via Bevy change detection and sends `textDocument/didChange`
/// automatically.
#[derive(Component, Debug, Clone)]
pub struct LspDocument {
    pub uri: Url,
    pub language_id: String,
    version: i32,
    pub(crate) pending_changes: Vec<TextDocumentContentChangeEvent>,
}

impl LspDocument {
    pub fn new(uri: Url, language_id: impl Into<String>) -> Self {
        Self {
            uri,
            version: 1,
            language_id: language_id.into(),
            pending_changes: Vec::new(),
        }
    }

    /// Queue an incremental change. [`crate::LspPlugin`] will flush it as
    /// `textDocument/didChange` on the next frame.
    pub fn push_change(&mut self, change: TextDocumentContentChangeEvent) {
        self.pending_changes.push(change);
    }

    /// Queue a full-document sync, discarding any queued incremental changes.
    pub fn push_full_sync(&mut self, text: String) {
        self.pending_changes.clear();
        self.pending_changes.push(TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text,
        });
    }

    pub fn version(&self) -> i32 {
        self.version
    }

    pub(crate) fn take_changes(&mut self) -> Option<(i32, Vec<TextDocumentContentChangeEvent>)> {
        if self.pending_changes.is_empty() {
            return None;
        }
        self.version += 1;
        Some((self.version, std::mem::take(&mut self.pending_changes)))
    }
}
