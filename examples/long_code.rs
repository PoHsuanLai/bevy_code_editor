//! Performance test example with a very large file (sqlite3.c - 150k+ lines)
//!
//! This example loads sqlite3.c to test scrolling performance, viewport culling,
//! and entity pooling with a massive codebase.

use bevy::prelude::*;
use bevy::window::{CursorIcon, SystemCursorIcon};
use bevy_code_editor::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Bevy Code Editor - Performance Test (sqlite3.c - 150k lines)".to_string(),
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
            content
        }
        Err(e) => {
            eprintln!("Failed to load {}: {}", file_path.display(), e);
            format!("// Failed to load sqlite3.c: {}\n// Make sure assets/sqlite3.c exists", e)
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
