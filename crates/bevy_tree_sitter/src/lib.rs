//! # bevy_tree_sitter
//!
//! Text-rendering-agnostic tree-sitter integration for Bevy. Provides:
//!
//! - [`SyntaxProvider`] — pluggable structural-highlight trait that returns
//!   capture names instead of colors.
//! - [`TreeSitterProvider`] — the canonical implementation, wrapping a
//!   tree-sitter parser/query/tree.
//! - [`Language`] / [`TreeSitterConfig`] — language descriptors.
//! - [`ParseTask`] / [`ParseCompleted`] — async parse machinery using
//!   `AsyncComputeTaskPool`.
//! - [`TreeSitterPlugin`] — registers the parse-task polling system.
//!
//! Consumers map [`HighlightRange::capture_name`] (e.g. `"keyword"`,
//! `"function.method"`) to whatever they need — colors for an editor,
//! structural metadata for an outline panel, semantic tags for an AI agent.

pub mod highlight;
pub mod language;
pub mod parse;
pub mod plugin;
pub mod prelude;
pub mod tree_sitter;

// Re-export the underlying tree-sitter crate so consumers can name
// `tree_sitter::Tree`, `tree_sitter::InputEdit`, etc. without taking a
// direct dep on the C-binding crate themselves.
pub use ::tree_sitter as ts;

pub use crate::highlight::{HighlightRange, SyntaxProvider};
pub use crate::language::{Language, TreeSitterConfig};
pub use crate::parse::{poll_parse_tasks, spawn_parse, ParseCompleted, ParseTask};
pub use crate::plugin::TreeSitterPlugin;
pub use crate::tree_sitter::{RopeReader, TreeSitterProvider, MAX_BYTES_TO_QUERY};
