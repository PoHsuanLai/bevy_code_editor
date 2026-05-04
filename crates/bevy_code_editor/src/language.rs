//! Language configuration for tree-sitter and LSP.

#[cfg(feature = "tree-sitter")]
use crate::syntax::TreeSitterProvider;

#[derive(Clone)]
pub struct Language {
    pub name: String,

    #[cfg(feature = "tree-sitter")]
    pub tree_sitter: Option<TreeSitterConfig>,

    #[cfg(feature = "lsp")]
    pub lsp_command: Option<(&'static str, &'static [&'static str])>,
}

#[cfg(feature = "tree-sitter")]
#[derive(Clone)]
pub struct TreeSitterConfig {
    pub grammar: tree_sitter::Language,
    pub highlights_query: String,
}

impl Language {
    /// Returns `None` if tree-sitter is not configured for this language.
    #[cfg(feature = "tree-sitter")]
    pub fn create_tree_sitter_provider(&self) -> Option<TreeSitterProvider> {
        let config = self.tree_sitter.as_ref()?;
        let mut provider = TreeSitterProvider::new();
        provider
            .set_query(&config.highlights_query, config.grammar.clone())
            .ok()?;
        Some(provider)
    }

    /// Returns `None` if LSP is not configured for this language.
    #[cfg(feature = "lsp")]
    pub fn lsp_command(&self) -> Option<(&'static str, &'static [&'static str])> {
        self.lsp_command
    }
}
