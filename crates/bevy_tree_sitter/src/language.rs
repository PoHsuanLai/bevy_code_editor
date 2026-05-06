//! Language descriptor + tree-sitter grammar configuration.

use bevy::prelude::*;

use crate::tree_sitter::TreeSitterProvider;

/// Language descriptor: name + optional tree-sitter grammar/query.
///
/// Not Reflect: `tree_sitter::Language` owns FFI-side state. LSP wiring lives
/// in the consumer crate so `bevy_tree_sitter` stays pure tree-sitter.
#[derive(Component, Clone)]
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

    /// Plain text — no tree-sitter wiring.
    pub fn plain(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            tree_sitter: None,
        }
    }

    /// Returns `None` if tree-sitter is unconfigured or the highlight query fails to compile.
    pub fn create_tree_sitter_provider(&self) -> Option<TreeSitterProvider> {
        let config = self.tree_sitter.as_ref()?;
        let mut provider = TreeSitterProvider::new();
        provider
            .set_query(&config.highlights_query, config.grammar.clone())
            .ok()?;
        Some(provider)
    }
}
