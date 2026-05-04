//! Language definitions used to wire tree-sitter grammars into a `TreeSitterProvider`.

use crate::tree_sitter::TreeSitterProvider;

/// A language descriptor — name plus optional tree-sitter configuration.
///
/// LSP wiring (when applicable) lives in the consumer crate; this struct
/// stays pure tree-sitter so consumers that don't need LSP don't pay for it.
#[derive(Clone)]
pub struct Language {
    pub name: String,
    pub tree_sitter: Option<TreeSitterConfig>,
}

#[derive(Clone)]
pub struct TreeSitterConfig {
    pub grammar: tree_sitter::Language,
    pub highlights_query: String,
}

impl Language {
    /// Construct a configured `TreeSitterProvider` for this language, or
    /// `None` if `tree_sitter` is `None` or the highlight query fails to compile.
    pub fn create_tree_sitter_provider(&self) -> Option<TreeSitterProvider> {
        let config = self.tree_sitter.as_ref()?;
        let mut provider = TreeSitterProvider::new();
        provider
            .set_query(&config.highlights_query, config.grammar.clone())
            .ok()?;
        Some(provider)
    }
}
