//! Component-driven tree-sitter integration for Bevy.
//!
//! Spawn [`Language`] + [`ParseSourceComp`] + [`SyntaxTree`] on an entity;
//! [`TreeSitterPlugin`] drives async parsing and writes results back via
//! `Changed<SyntaxTree>`. Consumers map [`HighlightRange::capture_name`]
//! (e.g. `"keyword"`, `"function.method"`) to colors, outline entries, etc.

pub mod highlight;
pub mod language;
pub mod parse;
pub mod plugin;
pub mod prelude;
pub mod tree_sitter;

/// Re-exported so consumers can name `tree_sitter::Tree`, `InputEdit`, etc.
/// without a direct dep on the C-binding crate.
pub use ::tree_sitter as ts;

pub use crate::highlight::{HighlightRange, SyntaxProvider};
pub use crate::language::{Language, TreeSitterConfig};
pub use crate::parse::{byte_to_point, ParseSource, ParseSourceComp, ParseTask, SyntaxTree};
pub use crate::plugin::{ParseSet, TreeSitterPlugin};
pub use crate::tree_sitter::{
    RopeReader, TreeSitterProvider, MAX_BYTES_TO_QUERY, SYNC_REPARSE_BYTE_LIMIT,
};
