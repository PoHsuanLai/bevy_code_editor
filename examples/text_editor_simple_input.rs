//! Minimal editable text field — proves `bevy_instanced_text_edit` works
//! standalone, without `bevy_code_editor`'s IDE features.
//!
//! Spawns one [`TextEditor`] entity with the engine's GPU rendering, plus
//! [`InstancedTextEditPlugin`] which gives you typed-character insertion, cursor
//! movement (via the editing-event observers), drag selection, scroll, and
//! clipboard copy out of the box.
//!
//! What you DON'T get here: line numbers, multi-cursor, folding, brackets,
//! syntax highlighting, LSP. That's the point — those live in the editor
//! crate one tier up.

use bevy::prelude::*;
use bevy_instanced_text_edit::{TextEditor, InstancedTextEditPlugin};
use bevy_instanced_text::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "bevy_instanced_text_edit — simple text input".to_string(),
                resolution: (800u32, 400u32).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(InstancedTextPlugins)
        .add_plugins(InstancedTextEditPlugin::default())
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);

    commands.spawn((
        TextEditor,
        FontConfig::from_size(20.0).with_line_height_multiplier(1.4),
        Name::new("simple-text-input"),
    ));
}
