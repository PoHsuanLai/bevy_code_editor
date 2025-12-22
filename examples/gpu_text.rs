//! GPU Text Performance Test
//!
//! This example uses the GPU-accelerated text rendering mode for better performance
//! with large files. It bypasses Bevy's Text2d layout system and uses a custom
//! glyph atlas + shader for rendering.

use bevy::prelude::*;
use bevy::window::{CursorIcon, SystemCursorIcon};
use bevy_code_editor::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Bevy Code Editor - GPU Text Rendering (Performance Test)".to_string(),
                resolution: (1400, 900).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(CodeEditorPlugin::default())
        .add_systems(Startup, setup_editor)
        .add_systems(Update, update_cursor_icon)
        .run();
}

fn setup_editor(mut state: ResMut<CodeEditorState>) {
    // Always focused in basic editor (no UI competing for input)
    state.is_focused = true;

    // Load sqlite3.c from assets folder
    let file_path = std::env::current_dir()
        .expect("Failed to get current directory")
        .join("assets/sqlite3.c");

    let content = match std::fs::read_to_string(&file_path) {
        Ok(content) => {
            println!("Loaded {} with {} lines", file_path.display(), content.lines().count());
            println!("Using GPU Text rendering mode (bypasses Bevy Text2d)");
            content
        }
        Err(e) => {
            eprintln!("Failed to load {}: {}", file_path.display(), e);
            eprintln!("Generating sample content instead...");
            // Generate sample content for testing
            let mut content = String::new();
            for i in 0..10000 {
                content.push_str(&format!("// Line {}: This is sample content for GPU text rendering test\n", i + 1));
            }
            content
        }
    };

    state.set_text(&content);
}

fn update_cursor_icon(
    state: Res<CodeEditorState>,
    mut commands: Commands,
    windows: Query<Entity, With<Window>>,
) {
    if let Ok(window_entity) = windows.single() {
        let icon = if state.is_focused {
            CursorIcon::System(SystemCursorIcon::Text)
        } else {
            CursorIcon::System(SystemCursorIcon::Default)
        };
        commands.entity(window_entity).insert(icon);
    }
}
