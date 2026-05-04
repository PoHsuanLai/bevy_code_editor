//! `TreeSitterPlugin` — registers the parse-completion machinery.

use bevy::prelude::*;

use crate::parse::{poll_parse_tasks, ParseCompleted};

/// Plugin that registers the [`ParseCompleted`] event and the system that
/// polls in-flight `ParseTask` components.
///
/// Spawning parses, applying their results to a `TreeSitterProvider`, and
/// driving content-version invalidation are all the consumer's job — this
/// crate stays unopinionated about how a host wires its highlighting pipeline.
#[derive(Default)]
pub struct TreeSitterPlugin;

impl Plugin for TreeSitterPlugin {
    fn build(&self, app: &mut App) {
        // Reflect not registered: every public type in this crate
        // (`ParseTask`, `ParseCompleted`, `Language`, `TreeSitterConfig`,
        // `TreeSitterProvider`) embeds `tree_sitter::Tree`/`Language`/`Parser`
        // or a `Task`, none of which implement `Reflect`. Skipping is
        // documented; the consumer crate keeps capture-name `Arc<str>`
        // strings reachable via its own reflected types.

        app.add_message::<ParseCompleted>();
        app.add_systems(Update, poll_parse_tasks);
    }
}
