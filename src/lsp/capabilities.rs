//! Server capability checking
//!
//! Prevents sending requests that the server doesn't support.

use lsp_types::*;
use std::sync::{Arc, RwLock};

/// Cached server capabilities
#[derive(Debug, Default, Clone)]
pub struct ServerCapabilitiesCache {
    inner: Arc<RwLock<Option<ServerCapabilities>>>,
}

impl ServerCapabilitiesCache {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
        }
    }

    /// Store server capabilities after initialization
    pub fn set(&self, capabilities: ServerCapabilities) {
        if let Ok(mut guard) = self.inner.write() {
            *guard = Some(capabilities);
        }
    }

    /// Check if server supports completion
    pub fn supports_completion(&self) -> bool {
        self.inner
            .read()
            .ok()
            .and_then(|guard| guard.as_ref().map(|c| c.completion_provider.is_some()))
            .unwrap_or(false)
    }

    /// Check if server supports hover
    pub fn supports_hover(&self) -> bool {
        self.inner
            .read()
            .ok()
            .and_then(|guard| {
                guard.as_ref().map(|c| match &c.hover_provider {
                    Some(HoverProviderCapability::Simple(b)) => *b,
                    Some(HoverProviderCapability::Options(_)) => true,
                    None => false,
                })
            })
            .unwrap_or(false)
    }

    /// Check if server supports goto definition
    pub fn supports_definition(&self) -> bool {
        self.inner
            .read()
            .ok()
            .and_then(|guard| {
                guard.as_ref().map(|c| match &c.definition_provider {
                    Some(OneOf::Left(b)) => *b,
                    Some(OneOf::Right(_)) => true,
                    None => false,
                })
            })
            .unwrap_or(false)
    }

    /// Check if server supports find references
    pub fn supports_references(&self) -> bool {
        self.inner
            .read()
            .ok()
            .and_then(|guard| {
                guard.as_ref().map(|c| match &c.references_provider {
                    Some(OneOf::Left(b)) => *b,
                    Some(OneOf::Right(_)) => true,
                    None => false,
                })
            })
            .unwrap_or(false)
    }

    /// Check if server supports document formatting
    pub fn supports_formatting(&self) -> bool {
        self.inner
            .read()
            .ok()
            .and_then(|guard| {
                guard.as_ref().map(|c| match &c.document_formatting_provider {
                    Some(OneOf::Left(b)) => *b,
                    Some(OneOf::Right(_)) => true,
                    None => false,
                })
            })
            .unwrap_or(false)
    }

    /// Check if server supports signature help
    pub fn supports_signature_help(&self) -> bool {
        self.inner
            .read()
            .ok()
            .and_then(|guard| guard.as_ref().map(|c| c.signature_help_provider.is_some()))
            .unwrap_or(false)
    }

    /// Check if server supports code actions
    pub fn supports_code_actions(&self) -> bool {
        self.inner
            .read()
            .ok()
            .and_then(|guard| {
                guard.as_ref().map(|c| match &c.code_action_provider {
                    Some(CodeActionProviderCapability::Simple(b)) => *b,
                    Some(CodeActionProviderCapability::Options(_)) => true,
                    None => false,
                })
            })
            .unwrap_or(false)
    }

    /// Check if server supports inlay hints
    pub fn supports_inlay_hints(&self) -> bool {
        self.inner
            .read()
            .ok()
            .and_then(|guard| {
                guard.as_ref().map(|c| match &c.inlay_hint_provider {
                    Some(OneOf::Left(b)) => *b,
                    Some(OneOf::Right(_)) => true,
                    None => false,
                })
            })
            .unwrap_or(false)
    }

    /// Get signature help trigger characters
    pub fn signature_help_triggers(&self) -> Vec<String> {
        self.inner
            .read()
            .ok()
            .and_then(|guard| {
                guard.as_ref().and_then(|c| {
                    c.signature_help_provider.as_ref().and_then(|p| {
                        p.trigger_characters.clone()
                    })
                })
            })
            .unwrap_or_default()
    }

    /// Get completion trigger characters
    pub fn completion_triggers(&self) -> Vec<String> {
        self.inner
            .read()
            .ok()
            .and_then(|guard| {
                guard.as_ref().and_then(|c| {
                    c.completion_provider.as_ref().and_then(|p| {
                        p.trigger_characters.clone()
                    })
                })
            })
            .unwrap_or_default()
    }

    /// Check if server supports document highlights
    pub fn supports_document_highlight(&self) -> bool {
        self.inner
            .read()
            .ok()
            .and_then(|guard| {
                guard.as_ref().map(|c| match &c.document_highlight_provider {
                    Some(OneOf::Left(b)) => *b,
                    Some(OneOf::Right(_)) => true,
                    None => false,
                })
            })
            .unwrap_or(false)
    }

    /// Check if server supports rename
    pub fn supports_rename(&self) -> bool {
        self.inner
            .read()
            .ok()
            .and_then(|guard| {
                guard.as_ref().map(|c| match &c.rename_provider {
                    Some(OneOf::Left(b)) => *b,
                    Some(OneOf::Right(_)) => true,
                    None => false,
                })
            })
            .unwrap_or(false)
    }

    /// Check if server supports prepare rename
    pub fn supports_prepare_rename(&self) -> bool {
        self.inner
            .read()
            .ok()
            .and_then(|guard| {
                guard.as_ref().map(|c| match &c.rename_provider {
                    Some(OneOf::Right(opts)) => opts.prepare_provider.unwrap_or(false),
                    _ => false,
                })
            })
            .unwrap_or(false)
    }
}
