//! Per-entity Component holding parsed LSP server capabilities. Until the
//! editor adapter sets it on `LspResponse::Initialized`, all `supports_*`
//! predicates return `false` and capability-gated sends are dropped.

use bevy::prelude::*;
use lsp_types::*;

#[derive(Component, Debug, Default, Clone)]
pub struct ServerCapabilities {
    inner: Option<lsp_types::ServerCapabilities>,
}

impl ServerCapabilities {
    pub fn new() -> Self {
        Self { inner: None }
    }

    pub fn set(&mut self, capabilities: lsp_types::ServerCapabilities) {
        self.inner = Some(capabilities);
    }

    /// `None` until [`crate::LspResponse::Initialized`] has been observed.
    pub fn get(&self) -> Option<&lsp_types::ServerCapabilities> {
        self.inner.as_ref()
    }

    pub fn supports_completion(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|c| c.completion_provider.is_some())
    }

    /// `completionItem/resolve` for lazy-loading docs and additional edits.
    /// Falls back to `false` so we don't fire requests servers would reject.
    pub fn supports_completion_resolve(&self) -> bool {
        self.inner
            .as_ref()
            .and_then(|c| c.completion_provider.as_ref())
            .and_then(|p| p.resolve_provider)
            .unwrap_or(false)
    }

    pub fn supports_hover(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|c| match &c.hover_provider {
                Some(HoverProviderCapability::Simple(b)) => *b,
                Some(HoverProviderCapability::Options(_)) => true,
                None => false,
            })
    }

    pub fn supports_definition(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|c| match &c.definition_provider {
                Some(OneOf::Left(b)) => *b,
                Some(OneOf::Right(_)) => true,
                None => false,
            })
    }

    pub fn supports_references(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|c| match &c.references_provider {
                Some(OneOf::Left(b)) => *b,
                Some(OneOf::Right(_)) => true,
                None => false,
            })
    }

    pub fn supports_formatting(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|c| match &c.document_formatting_provider {
                Some(OneOf::Left(b)) => *b,
                Some(OneOf::Right(_)) => true,
                None => false,
            })
    }

    pub fn supports_signature_help(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|c| c.signature_help_provider.is_some())
    }

    pub fn supports_code_actions(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|c| match &c.code_action_provider {
                Some(CodeActionProviderCapability::Simple(b)) => *b,
                Some(CodeActionProviderCapability::Options(_)) => true,
                None => false,
            })
    }

    pub fn supports_inlay_hints(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|c| match &c.inlay_hint_provider {
                Some(OneOf::Left(b)) => *b,
                Some(OneOf::Right(_)) => true,
                None => false,
            })
    }

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

    /// Negotiated per LSP 3.17+. Spec default (omitted field) is UTF-16.
    pub fn position_encoding(&self) -> crate::pos::PositionEncoding {
        use lsp_types::PositionEncodingKind;
        let raw = self.inner.as_ref().and_then(|c| c.position_encoding.clone());
        match raw {
            Some(k) if k == PositionEncodingKind::UTF8 => crate::pos::PositionEncoding::Utf8,
            Some(k) if k == PositionEncodingKind::UTF32 => crate::pos::PositionEncoding::Utf32,
            _ => crate::pos::PositionEncoding::Utf16,
        }
    }

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

    pub fn supports_document_highlight(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|c| match &c.document_highlight_provider {
                Some(OneOf::Left(b)) => *b,
                Some(OneOf::Right(_)) => true,
                None => false,
            })
    }

    pub fn supports_rename(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|c| match &c.rename_provider {
                Some(OneOf::Left(b)) => *b,
                Some(OneOf::Right(_)) => true,
                None => false,
            })
    }

    pub fn supports_prepare_rename(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|c| match &c.rename_provider {
                Some(OneOf::Right(opts)) => opts.prepare_provider.unwrap_or(false),
                _ => false,
            })
    }
}
