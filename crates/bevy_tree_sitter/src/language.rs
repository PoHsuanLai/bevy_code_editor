//! Language definitions used to wire tree-sitter grammars into a `TreeSitterProvider`.

use crate::tree_sitter::TreeSitterProvider;

/// A language descriptor — name plus optional tree-sitter configuration.
///
/// LSP wiring (when applicable) lives in the consumer crate; this struct
/// stays pure tree-sitter so consumers that don't need LSP don't pay for it.
///
/// Hosts add their preferred grammar crate directly. Common pattern:
///
/// ```rust,ignore
/// // Cargo.toml: tree-sitter-rust = "0.23"
/// use bevy_tree_sitter::Language;
/// let rust = Language::from_grammar(
///     "rust",
///     tree_sitter_rust::LANGUAGE.into(),
///     tree_sitter_rust::HIGHLIGHTS_QUERY,
/// );
/// ```
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
    /// Build a `Language` with tree-sitter configured. The most common path —
    /// hosts wire one of these per grammar they want to support.
    pub fn from_grammar(
        name: impl Into<String>,
        grammar: tree_sitter::Language,
        highlights_query: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            tree_sitter: Some(TreeSitterConfig {
                grammar,
                highlights_query: highlights_query.into(),
            }),
        }
    }

    /// A language descriptor with no tree-sitter wiring (plain text).
    pub fn plain(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            tree_sitter: None,
        }
    }

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
