//! Two `CodeEditor` entities side-by-side, each with its own state.
//!
//! Validates that the editor crate is genuinely multi-instance:
//!   - Two editors coexist without panicking on `single()` queries.
//!   - Each tracks its own cursor, selection, fold regions, and bracket
//!     match state.
//!   - Click-to-focus routes keyboard input to the editor under the cursor
//!     via `bevy::input_focus::InputFocus`.
//!
//! Layout: the window is split horizontally; the left editor renders on
//! `RenderLayers::layer(0)` (default), the right editor on
//! `RenderLayers::layer(1)`. Each editor has its own viewport rect and a
//! dedicated 2D camera; mouse hit-testing maps screen coordinates back to
//! the editor whose viewport contains them.
//!
//! Run: `cargo run --example multi_editor`

use bevy::prelude::*;
use bevy_camera::visibility::RenderLayers;
use bevy_code_editor::prelude::*;

const WINDOW_WIDTH: u32 = 1600;
const WINDOW_HEIGHT: u32 = 900;
const PANEL_WIDTH: u32 = WINDOW_WIDTH / 2;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "bevy_code_editor — multi-editor".into(),
                resolution: (WINDOW_WIDTH, WINDOW_HEIGHT).into(),
                ..default()
            }),
            ..default()
        }))
        // Embedded mode: explicitly compose the engine plugins, the
        // interaction plugin, and the editor plugin. We skip `EditorUiPlugin`
        // (no global gutter/separator/camera) because each editor here
        // carries its own camera + render layer pair.
        .add_plugins((
            TextEnginePlugins,
            TextInteractionPlugin,
            CodeEditorPlugin,
        ))
        .add_systems(Startup, (despawn_default_editor, spawn_two_editors).chain())
        .run();
}

/// `CodeEditorPlugin` auto-spawns a default editor on `Startup`. Despawn
/// it before our custom spawn runs so we end up with exactly two editors.
fn despawn_default_editor(
    mut commands: Commands,
    editors: Query<Entity, With<CodeEditor>>,
) {
    for e in editors.iter() {
        commands.entity(e).despawn();
    }
}

fn spawn_two_editors(
    mut commands: Commands,
    mut input_focus: ResMut<bevy::input_focus::InputFocus>,
) {
    // Two cameras, one per editor, with non-overlapping viewport rects so
    // each occupies half of the window.
    let left_layer = RenderLayers::layer(0);
    let right_layer = RenderLayers::layer(1);

    commands.spawn((
        Camera2d,
        Camera {
            order: 0,
            viewport: Some(bevy::camera::Viewport {
                physical_position: UVec2::new(0, 0),
                physical_size: UVec2::new(PANEL_WIDTH, WINDOW_HEIGHT),
                ..default()
            }),
            ..default()
        },
        left_layer.clone(),
        Name::new("LeftCamera"),
    ));
    commands.spawn((
        Camera2d,
        Camera {
            order: 1,
            viewport: Some(bevy::camera::Viewport {
                physical_position: UVec2::new(PANEL_WIDTH, 0),
                physical_size: UVec2::new(PANEL_WIDTH, WINDOW_HEIGHT),
                ..default()
            }),
            ..default()
        },
        right_layer.clone(),
        Name::new("RightCamera"),
    ));

    let left = commands
        .spawn((
            CodeEditor,
            TextViewViewport {
                width: PANEL_WIDTH,
                height: WINDOW_HEIGHT,
                hit_test_position: Vec2::new(0.0, 0.0),
                ..default()
            },
            left_layer,
            Name::new("LeftEditor"),
        ))
        .id();
    let right = commands
        .spawn((
            CodeEditor,
            TextViewViewport {
                width: PANEL_WIDTH,
                height: WINDOW_HEIGHT,
                hit_test_position: Vec2::new(PANEL_WIDTH as f32, 0.0),
                ..default()
            },
            right_layer,
            Name::new("RightEditor"),
        ))
        .id();

    input_focus.set(left);

    info!(
        "Spawned multi-editor demo: left = {:?}, right = {:?}. Click either side to focus.",
        left, right
    );
}
