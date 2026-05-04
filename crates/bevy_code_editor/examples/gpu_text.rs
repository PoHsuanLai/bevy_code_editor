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
        .add_plugins(CodeEditorPlugin)
        .add_plugins(EditorUiPlugin::default())
        .add_systems(PostStartup, setup_editor)
        .add_systems(Update, update_cursor_icon)
        .run();
}

fn setup_editor(
    mut editor_query: Query<
        (
            Entity,
            &mut CursorState,
            &mut TextViewState,
            &mut EditHistoryState,
            &mut SelectionState,
            &mut SyntaxCacheState,
        ),
        With<CodeEditor>,
    >,
    mut input_focus: ResMut<bevy::input_focus::InputFocus>,
) {
    let Ok((entity, mut cursor, mut tv, mut hist, mut sel, mut syntax_cache)) =
        editor_query.single_mut()
    else {
        return;
    };

    // Always focused in basic editor (no UI competing for input)
    input_focus.set(entity);

    // Load sqlite3.c from assets folder
    let file_path = std::env::current_dir()
        .expect("Failed to get current directory")
        .join("assets/sqlite3.c");

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
            // Generate sample content for testing
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

    hist.set_text(&mut sel, &mut syntax_cache, &mut cursor, &mut tv, &content);
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
