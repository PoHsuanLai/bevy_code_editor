//! Convenient re-exports for typical consumer use.

pub use crate::highlight::{HighlightRange, SyntaxProvider};
pub use crate::language::{Language, TreeSitterConfig};
pub use crate::parse::{byte_to_point, ParseSource, ParseSourceComp, SyntaxTree};
pub use crate::plugin::{ParseSet, TreeSitterPlugin};
pub use crate::tree_sitter::TreeSitterProvider;
