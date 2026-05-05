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
        .add_plugins(CodeEditorPlugin::standalone())
        .add_systems(PostStartup, setup_editor)
        .add_systems(Update, update_cursor_icon)
        .run();
}

fn setup_editor(
    editor_query: Query<Entity, With<CodeEditor>>,
    mut input_focus: ResMut<bevy::input_focus::InputFocus>,
    mut set_text_writer: MessageWriter<bevy_text_editor::SetTextRequested>,
) {
    let Ok(entity) = editor_query.single() else {
        return;
    };
    input_focus.set(entity);

    let file_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/sqlite3.c");
    let content = match std::fs::read_to_string(&file_path) {
        Ok(content) => {
            println!(
                "Loaded {} with {} lines",
                file_path.display(),
                content.lines().count()
            );
            println!("Using GPU Text rendering mode (bypasses Bevy Text2d)");
            content
        }
        Err(e) => {
            eprintln!("Failed to load {}: {}", file_path.display(), e);
            eprintln!("Generating sample content instead...");
            let mut content = String::new();
            for i in 0..10000 {
                content.push_str(&format!(
                    "// Line {}: This is sample content for GPU text rendering test\n",
                    i + 1
                ));
            }
            content
        }
    };

    set_text_writer.write(bevy_text_editor::SetTextRequested {
        entity,
        text: content,
    });
}

fn update_cursor_icon(
    editor_query: Query<Entity, With<CodeEditor>>,
    input_focus: Res<bevy::input_focus::InputFocus>,
    mut commands: Commands,
    windows: Query<Entity, With<Window>>,
) {
    let Ok(editor_entity) = editor_query.single() else {
        return;
    };

    if let Ok(window_entity) = windows.single() {
        let is_focused = input_focus.get() == Some(editor_entity);
        let icon = if is_focused {
            CursorIcon::System(SystemCursorIcon::Text)
        } else {
            CursorIcon::System(SystemCursorIcon::Default)
        };
        commands.entity(window_entity).insert(icon);
    }
}
