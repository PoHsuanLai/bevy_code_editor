//! Language configuration API
//!
//! Provides a simple struct for configuring tree-sitter and LSP for a language.
//! Users create their own language configurations.
//!
//! # Example
//!
//! ```rust,ignore
//! use bevy_code_editor::prelude::*;
//!
//! fn setup(mut syntax: ResMut<SyntaxResource>) {
//!     // Define your language
//!     let rust = Language {
//!         name: "rust",
//!         tree_sitter: Some(TreeSitterConfig {
//!             grammar: tree_sitter_rust::LANGUAGE.into(),
//!             highlights_query: tree_sitter_rust::HIGHLIGHTS_QUERY,
//!         }),
//!         lsp_command: Some(("rust-analyzer", &[])),
//!     };
//!
//!     // Use it
//!     if let Some(provider) = rust.create_tree_sitter_provider() {
//!         syntax.set_provider(provider);
//!     }
//! }
//! ```
//!
//! # Organizing Multiple Languages
//!
//! For applications that support multiple languages, you can create a module
//! with language definitions:
//!
//! ```rust,ignore
//! // In your app's languages.rs module
//! use bevy_code_editor::prelude::*;
//!
//! pub fn rust() -> Language {
//!     Language {
//!         name: "rust",
//!         tree_sitter: Some(TreeSitterConfig {
//!             grammar: tree_sitter_rust::LANGUAGE.into(),
//!             highlights_query: tree_sitter_rust::HIGHLIGHTS_QUERY,
//!         }),
//!         lsp_command: Some(("rust-analyzer", &[])),
//!     }
//! }
//!
//! pub fn python() -> Language {
//!     Language {
//!         name: "python",
//!         tree_sitter: Some(TreeSitterConfig {
//!             grammar: tree_sitter_python::LANGUAGE.into(),
//!             highlights_query: tree_sitter_python::HIGHLIGHTS_QUERY,
//!         }),
//!         lsp_command: Some(("pyright-langserver", &["--stdio"])),
//!     }
//! }
//!
//! // Then use in your systems:
//! fn setup_for_rust(mut syntax: ResMut<SyntaxResource>) {
//!     if let Some(provider) = languages::rust().create_tree_sitter_provider() {
//!         syntax.set_provider(provider);
//!     }
//! }
//! ```

#[cfg(feature = "tree-sitter")]
use crate::syntax::TreeSitterProvider;

/// A language configuration containing tree-sitter and LSP settings
#[derive(Clone)]
pub struct Language {
    /// Language name (e.g., "rust", "python")
    pub name: &'static str,

    /// Tree-sitter configuration (optional)
    #[cfg(feature = "tree-sitter")]
    pub tree_sitter: Option<TreeSitterConfig>,

    /// LSP server command and arguments (optional)
    #[cfg(feature = "lsp")]
    pub lsp_command: Option<(&'static str, &'static [&'static str])>,
}

/// Tree-sitter grammar and query configuration
#[cfg(feature = "tree-sitter")]
#[derive(Clone)]
pub struct TreeSitterConfig {
    /// The tree-sitter grammar
    pub grammar: tree_sitter::Language,

    /// The highlights query string
    pub highlights_query: &'static str,
}

impl Language {
    /// Create a tree-sitter provider from this language configuration
    ///
    /// Returns `None` if tree-sitter is not configured for this language.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// if let Some(provider) = language.create_tree_sitter_provider() {
    ///     syntax.set_provider(provider);
    /// }
    /// ```
    #[cfg(feature = "tree-sitter")]
    pub fn create_tree_sitter_provider(&self) -> Option<TreeSitterProvider> {
        let config = self.tree_sitter.as_ref()?;
        let mut provider = TreeSitterProvider::new();
        provider
            .set_query(config.highlights_query, config.grammar.clone())
            .ok()?;
        Some(provider)
    }

    /// Get the LSP command and arguments
    ///
    /// Returns `None` if LSP is not configured for this language.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// if let Some((cmd, args)) = language.lsp_command() {
    ///     lsp_client.start(cmd, args)?;
    /// }
    /// ```
    #[cfg(feature = "lsp")]
    pub fn lsp_command(&self) -> Option<(&'static str, &'static [&'static str])> {
        self.lsp_command
    }
}
