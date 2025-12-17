//! 3D Code Editor Example
//!
//! This example demonstrates the 3D rendering mode for the code editor,
//! where code is displayed as extruded 3D text meshes that can be viewed
//! from different angles.
//!
//! Run with:
//!     cargo run --example editor_3d --features render3d
//!
//! Controls:
//! - Type to edit code
//! - Arrow keys to move cursor
//! - Mouse drag to orbit camera
//! - Scroll to zoom

use bevy::prelude::*;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy_code_editor::prelude::*;
use bevy_code_editor::render3d::{
    setup_3d_materials, setup_3d_scene, update_3d_code_lines, update_3d_cursor,
    update_3d_line_numbers, animate_3d_cursor,
};
use bevy_fontmesh::FontMeshPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "3D Code Editor".to_string(),
                resolution: (1400, 900).into(),
                ..default()
            }),
            ..default()
        }))
        // Add FontMesh plugin for 3D text rendering
        .add_plugins(FontMeshPlugin)
        // Add code editor plugin configured for 3D (state/input only, no 2D rendering)
        .add_plugins(CodeEditorPlugin::for_3d())
        // Setup systems
        .add_systems(Startup, (setup_3d_scene, setup_sample_code))
        .add_systems(
            Update,
            (
                setup_3d_materials,
                update_3d_code_lines,
                update_3d_cursor,
                update_3d_line_numbers,
                animate_3d_cursor,
                camera_controller,
            ),
        )
        .run();
}

fn setup_sample_code(mut state: ResMut<CodeEditorState>) {
    let sample_code = r#"// 3D Code Editor Demo
// Navigate with arrow keys, type to edit

fn main() {
    println!("Hello, 3D World!");

    let numbers = vec![1, 2, 3, 4, 5];

    for n in numbers {
        if n % 2 == 0 {
            println!("{} is even", n);
        } else {
            println!("{} is odd", n);
        }
    }
}

struct Point {
    x: f32,
    y: f32,
}

impl Point {
    fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    fn distance(&self, other: &Point) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}
"#;

    state.set_text(sample_code);
}

/// Simple orbit camera controller
fn camera_controller(
    mut camera_query: Query<&mut Transform, With<Camera3d>>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: MessageReader<MouseMotion>,
    mut scroll_events: MessageReader<MouseWheel>,
) {
    let Ok(mut transform) = camera_query.single_mut() else {
        return;
    };

    // Get current camera position relative to target
    let target = Vec3::new(5.0, -5.0, 0.0); // Center of code view
    let relative_pos = transform.translation - target;
    let distance = relative_pos.length();

    // Orbit with right mouse button
    if mouse_button.pressed(MouseButton::Right) {
        for event in mouse_motion.read() {
            let sensitivity = 0.005;

            // Horizontal rotation (around Y axis)
            let yaw = Quat::from_rotation_y(-event.delta.x * sensitivity);

            // Apply rotations
            let new_pos = yaw * relative_pos;
            let new_pos = target + new_pos;

            transform.translation = new_pos;
            transform.look_at(target, Vec3::Y);

            // Apply pitch
            let right = transform.right();
            let pitch_rotation = Quat::from_axis_angle(*right, -event.delta.y * sensitivity);
            let new_pos = pitch_rotation * (transform.translation - target) + target;

            // Clamp vertical angle
            let up_dot = (new_pos - target).normalize().dot(Vec3::Y);
            if up_dot.abs() < 0.95 {
                transform.translation = new_pos;
                transform.look_at(target, Vec3::Y);
            }
        }
    } else {
        mouse_motion.clear();
    }

    // Zoom with scroll wheel
    for event in scroll_events.read() {
        let zoom_speed = 0.5;
        let zoom = 1.0 - event.y * zoom_speed;
        let new_distance = (distance * zoom).clamp(3.0, 30.0);

        let direction = relative_pos.normalize();
        transform.translation = target + direction * new_distance;
    }
}
