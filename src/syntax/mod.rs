//! Syntax highlighting module
//!
//! Provides pluggable syntax highlighting through the SyntaxProvider trait.
//! Currently supports tree-sitter, but can be extended with other providers.

pub mod highlighter;

#[cfg(feature = "tree-sitter")]
pub mod tree_sitter;

// Re-export main types
pub use highlighter::{map_highlight_color, SyntaxProvider};

#[cfg(feature = "tree-sitter")]
pub use tree_sitter::TreeSitterProvider;
