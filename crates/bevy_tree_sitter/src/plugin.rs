//! `TreeSitterPlugin` — drives the per-entity parse pipeline.

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;

use crate::pipeline::parse_dirty;

/// Contains `parse_dirty`. Schedule before to sync a `ParseSource`'s buffer;
/// after to react to `Changed<SyntaxTree>`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParseSet;

/// Registers `parse_dirty`, which drives the per-entity async parse pipeline.
#[derive(Default)]
pub struct TreeSitterPlugin;

impl Plugin for TreeSitterPlugin {
    fn build(&self, app: &mut App) {
        // Reflect not registered: all public types embed `tree_sitter::Tree`,
        // `Language`, `Parser`, `Task`, or `dyn Trait` — none implement Reflect.
        app.add_systems(Update, parse_dirty.in_set(ParseSet));
    }
}
