//! Component-driven tree-sitter integration for Bevy.
//!
//! Parses text into syntax trees on a background thread and exposes the
//! results as ECS components. This crate is rendering-agnostic — it produces
//! structured data ([`SyntaxTree`], [`HighlightRange`]); what you do with
//! that data (color tokens, build an outline panel, feed an AI agent) is up
//! to the host.
//!
//! ## Concepts
//!
//! Attach these components to an entity to opt into async parsing:
//!
//! - **[`TreeSitterGrammar`]** — which grammar to use. Construct with
//!   `TreeSitterGrammar::new(grammar, highlights_query)`.
//! - **[`ParseSourceComp`]** — wraps a [`ParseSource`] implementor that
//!   provides the text content and a version counter. The plugin polls
//!   `content_version()` each frame; a bump triggers a re-parse.
//! - **[`SyntaxTree`]** — written back by the plugin when parsing completes.
//!   Subscribe with `Changed<SyntaxTree>` to react to new trees.
//!
//! To get highlight ranges from a tree, call [`highlight_ranges`] with the
//! tree, rope, and compiled query. Each [`HighlightRange`] carries a
//! [`HighlightRange::capture_name`] (e.g. `"keyword"`, `"function.method"`,
//! `"comment"`) — map these to colors, decorations, or whatever the host needs.
//!
//! ## What this crate does NOT provide
//!
//! - Color mapping or theming — hosts decide what `"keyword"` looks like
//! - Rendering — feed the ranges to `bevy_instanced_text` `LineStyles` or any other renderer
//! - Folding or code navigation — those are host concerns
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use bevy_tree_sitter::{TreeSitterGrammar, TreeSitterPlugin};
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(TreeSitterPlugin)
//!     .run();
//! ```

pub mod highlight;
pub mod language;
pub mod pipeline;
pub mod plugin;
pub mod prelude;
pub mod tree_sitter;

/// Re-exported so consumers can name `tree_sitter::Tree`, `InputEdit`, etc.
/// without a direct dep on the C-binding crate.
pub use ::tree_sitter as ts;

pub use crate::highlight::{highlight_ranges, HighlightRange};
pub use crate::language::TreeSitterGrammar;
pub use crate::pipeline::{byte_to_point, ParseSource, ParseSourceComp, SyntaxTree};
pub use crate::plugin::{ParseSet, TreeSitterPlugin};
pub use crate::tree_sitter::TreeSitterProvider;
