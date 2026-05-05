//! Server capability checking.
//!
//! Prevents sending requests that the server doesn't support.
//!
//! [`ServerCapabilities`] is a per-entity Component populated when the
//! `LspResponse::Initialized` is drained from [`crate::LspClient`]. The reader
//! thread no longer mutates capability state directly; the editor adapter (or
//! any consumer) drives this via the response channel.

use bevy::prelude::*;
use lsp_types::*;

/// Server capabilities Component.
///
/// Holds an optional [`lsp_types::ServerCapabilities`] populated by the editor
/// adapter when it receives an [`crate::LspResponse::Initialized`]. Until that
/// happens, all `supports_*` predicates return `false` and the client's
/// capability-aware send-gating short-circuits everything except `Initialize`,
/// `Initialized`, `DidOpen`, `DidChange`, and `ExecuteCommand`.
#[derive(Component, Debug, Default, Clone)]
pub struct ServerCapabilities {
    inner: Option<lsp_types::ServerCapabilities>,
}

impl ServerCapabilities {
    pub fn new() -> Self {
        Self { inner: None }
    }

    /// Store server capabilities after initialization.
    pub fn set(&mut self, capabilities: lsp_types::ServerCapabilities) {
        self.inner = Some(capabilities);
    }

    /// Reference to the raw capabilities (None until `Initialized` is observed).
    pub fn get(&self) -> Option<&lsp_types::ServerCapabilities> {
        self.inner.as_ref()
    }

    /// Check if server supports completion
    pub fn supports_completion(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|c| c.completion_provider.is_some())
    }

    /// Check if server supports `completionItem/resolve` (the lazy-load
    /// path for documentation, additional edits, etc.). Falls back to
    /// `false` when the provider doesn't advertise resolve support so we
    /// don't fire requests rust-analyzer would reject as unsupported.
    pub fn supports_completion_resolve(&self) -> bool {
        self.inner
            .as_ref()
            .and_then(|c| c.completion_provider.as_ref())
            .and_then(|p| p.resolve_provider)
            .unwrap_or(false)
    }

    /// Check if server supports hover
    pub fn supports_hover(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|c| match &c.hover_provider {
                Some(HoverProviderCapability::Simple(b)) => *b,
                Some(HoverProviderCapability::Options(_)) => true,
                None => false,
            })
    }

    /// Check if server supports goto definition
    pub fn supports_definition(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|c| match &c.definition_provider {
                Some(OneOf::Left(b)) => *b,
                Some(OneOf::Right(_)) => true,
                None => false,
            })
    }

    /// Check if server supports find references
    pub fn supports_references(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|c| match &c.references_provider {
                Some(OneOf::Left(b)) => *b,
                Some(OneOf::Right(_)) => true,
                None => false,
            })
    }

    /// Check if server supports document formatting
    pub fn supports_formatting(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|c| match &c.document_formatting_provider {
                Some(OneOf::Left(b)) => *b,
                Some(OneOf::Right(_)) => true,
                None => false,
            })
    }

    /// Check if server supports signature help
    pub fn supports_signature_help(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|c| c.signature_help_provider.is_some())
    }

    /// Check if server supports code actions
    pub fn supports_code_actions(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|c| match &c.code_action_provider {
                Some(CodeActionProviderCapability::Simple(b)) => *b,
                Some(CodeActionProviderCapability::Options(_)) => true,
                None => false,
            })
    }

    /// Check if server supports inlay hints
    pub fn supports_inlay_hints(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|c| match &c.inlay_hint_provider {
                Some(OneOf::Left(b)) => *b,
                Some(OneOf::Right(_)) => true,
                None => false,
            })
    }

    /// Get signature help trigger characters
    pub fn signature_help_triggers(&self) -> Vec<String> {
        self.inner
            .as_ref()
            .and_then(|c| {
                c.signature_help_provider
                    .as_ref()
                    .and_then(|p| p.trigger_characters.clone())
            })
            .unwrap_or_default()
    }

    /// Negotiated position encoding. LSP 3.17+: servers advertise their
    /// preferred encoding in `position_encoding`; the spec default
    /// (omitted field) is UTF-16. We honor the server's choice when it
    /// advertises one.
    pub fn position_encoding(&self) -> crate::pos::PositionEncoding {
        use lsp_types::PositionEncodingKind;
        let raw = self.inner.as_ref().and_then(|c| c.position_encoding.clone());
        match raw {
            Some(k) if k == PositionEncodingKind::UTF8 => crate::pos::PositionEncoding::Utf8,
            Some(k) if k == PositionEncodingKind::UTF32 => crate::pos::PositionEncoding::Utf32,
            _ => crate::pos::PositionEncoding::Utf16,
        }
    }

    /// Get completion trigger characters
    pub fn completion_triggers(&self) -> Vec<String> {
        self.inner
            .as_ref()
            .and_then(|c| {
                c.completion_provider
                    .as_ref()
                    .and_then(|p| p.trigger_characters.clone())
            })
            .unwrap_or_default()
    }

    /// Check if server supports document highlights
    pub fn supports_document_highlight(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|c| match &c.document_highlight_provider {
                Some(OneOf::Left(b)) => *b,
                Some(OneOf::Right(_)) => true,
                None => false,
            })
    }

    /// Check if server supports rename
    pub fn supports_rename(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|c| match &c.rename_provider {
                Some(OneOf::Left(b)) => *b,
                Some(OneOf::Right(_)) => true,
                None => false,
            })
    }

    /// Check if server supports prepare rename
    pub fn supports_prepare_rename(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|c| match &c.rename_provider {
                Some(OneOf::Right(opts)) => opts.prepare_provider.unwrap_or(false),
                _ => false,
            })
    }
}
