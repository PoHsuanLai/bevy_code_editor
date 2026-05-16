//! Tree-sitter grammar configuration component.

use bevy_ecs::prelude::*;
use crate::tree_sitter::TreeSitterProvider;

/// Tree-sitter grammar + highlight query for an entity.
///
/// Insert this component to opt into async parsing and highlight queries.
/// Omit it for plain-text entities.
///
/// Not Reflect: `tree_sitter::Language` owns FFI-side state.
#[derive(Component, Clone)]
pub struct TreeSitterGrammar {
    pub grammar: tree_sitter::Language,
    pub highlights_query: String,
}

impl TreeSitterGrammar {
    pub fn new(grammar: tree_sitter::Language, highlights_query: impl Into<String>) -> Self {
        Self {
            grammar,
            highlights_query: highlights_query.into(),
        }
    }

    /// Build a [`TreeSitterProvider`] from this grammar. Returns `None` if the
    /// highlight query fails to compile.
    pub fn create_provider(&self) -> Option<TreeSitterProvider> {
        let mut provider = TreeSitterProvider::new();
        provider
            .set_query(&self.highlights_query, self.grammar.clone())
            .ok()?;
        Some(provider)
    }
}
