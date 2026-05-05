//! `TreeSitterPlugin` — drives the per-entity parse pipeline.

use bevy::prelude::*;

use crate::parse::parse_dirty;

/// System set that contains [`parse_dirty`]. Consumers can schedule their
/// own systems `.before(ParseSet)` (e.g. to sync a `ParseSource`'s
/// underlying buffer from a domain Component before the parser reads it)
/// or `.after(ParseSet)` (e.g. to react to `Changed<SyntaxTree>`).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParseSet;

/// Plugin that registers [`parse_dirty`], the system that walks every
/// entity with a [`crate::Language`] + [`crate::ParseSourceComp`] +
/// [`crate::SyntaxTree`] triple, kicks off async parses when the source's
/// content version outruns the tree's, and writes results back into the
/// `SyntaxTree` Component.
///
/// Consumers don't interact with this plugin directly — they spawn the
/// three Components on their entity and react to `Changed<SyntaxTree>`
/// from their own systems.
#[derive(Default)]
pub struct TreeSitterPlugin;

impl Plugin for TreeSitterPlugin {
    fn build(&self, app: &mut App) {
        // Reflect not registered: every public type in this crate
        // (`Language`, `SyntaxTree`, `ParseSourceComp`, `ParseTask`,
        // `TreeSitterProvider`) embeds `tree_sitter::Tree`/`Language`/
        // `Parser` or a `Task` / `dyn Trait`, none of which implement
        // `Reflect`. Skipping is documented; the consumer crate keeps
        // capture-name `Arc<str>` strings reachable via its own reflected
        // types.

        app.add_systems(Update, parse_dirty.in_set(ParseSet));
    }
}
