//! Convenient re-exports for typical consumer use.

pub use crate::highlight::{HighlightRange, SyntaxProvider};
pub use crate::language::{Language, TreeSitterConfig};
pub use crate::parse::{spawn_parse, ParseCompleted, ParseTask};
pub use crate::plugin::TreeSitterPlugin;
pub use crate::tree_sitter::TreeSitterProvider;
