//! # bevy_tree_sitter
//!
//! Component-driven tree-sitter integration for Bevy. Provides:
//!
//! - [`SyntaxProvider`] — pluggable structural-highlight trait that returns
//!   capture names instead of colors.
//! - [`TreeSitterProvider`] — the canonical implementation, wrapping a
//!   tree-sitter parser/query/tree.
//! - [`Language`] — Component holding a grammar + highlight query.
//! - [`SyntaxTree`] — Component holding the parsed tree, written by the
//!   parse pipeline. Consumers filter on `Changed<SyntaxTree>` to react.
//! - [`ParseSource`] / [`ParseSourceComp`] — trait Component the consumer
//!   implements to plug their buffer (`Rope`, `String`, etc.) into the
//!   parser.
//! - [`ParseTask`] — async-parse worker, attached to a child entity by
//!   the parse pipeline (private detail unless you're inspecting tasks).
//! - [`TreeSitterPlugin`] — registers the parse-driving system.
//!
//! Consumers map [`HighlightRange::capture_name`] (e.g. `"keyword"`,
//! `"function.method"`) to whatever they need — colors for an editor,
//! structural metadata for an outline panel, semantic tags for an AI agent.
//!
//! ## Usage sketch
//!
//! ```rust,ignore
//! use bevy::prelude::*;
//! use bevy_tree_sitter::prelude::*;
//!
//! struct MyParseSource(Arc<RwLock<MyBuffer>>);
//! impl ParseSource for MyParseSource {
//!     fn content_version(&self) -> u64 { self.0.read().unwrap().version }
//!     fn snapshot(&self) -> ropey::Rope { self.0.read().unwrap().rope.clone() }
//! }
//!
//! fn setup(mut commands: Commands) {
//!     commands.spawn((
//!         Language::from_grammar("rust", /* ... */, /* ... */),
//!         SyntaxTree::default(),
//!         ParseSourceComp::new(MyParseSource(/* ... */)),
//!     ));
//! }
//!
//! fn react(q: Query<&SyntaxTree, Changed<SyntaxTree>>) {
//!     for tree in q.iter() { /* invalidate caches */ }
//! }
//! ```

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
pub use crate::parse::{byte_to_point, ParseSource, ParseSourceComp, ParseTask, SyntaxTree};
pub use crate::plugin::{ParseSet, TreeSitterPlugin};
pub use crate::tree_sitter::{RopeReader, TreeSitterProvider, MAX_BYTES_TO_QUERY};
