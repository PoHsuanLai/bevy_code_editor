//! Basic code editor example
//!
//! Demonstrates the bevy_code_editor plugin with built-in input handling.
//!
//! The plugin automatically handles:
//! - Text input (typing characters)
//! - Backspace/Delete
//! - Arrow keys for navigation
//! - Selection (Shift + arrows)
//! - Copy/Paste (Ctrl+C/V)
//! - Undo/Redo (Ctrl+Z/Y)
//! - And more!
//!
//! You can customize keybindings via the Keybindings resource.

use bevy::prelude::*;
use bevy::window::{CursorIcon, SystemCursorIcon};
use bevy_code_editor::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Bevy Code Editor".to_string(),
                resolution: (1400, 900).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(CodeEditorPlugin::standalone())
        .add_systems(Startup, setup_camera)
        .add_systems(PostStartup, setup_editor)
        .add_systems(Update, update_cursor_icon)
        .run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Camera {
            clear_color: ClearColorConfig::Custom(ThemeConfig::default().background),
            ..default()
        },
    ));
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

    // Set initial Python code
    let initial_text = r#"#!/usr/bin/env python3
"""
Example Python script demonstrating the Bevy Code Editor.

This editor features:
- GPU-accelerated rendering
- Efficient rope data structure
- Built-in keybindings (customizable!)
- Selection support
- Syntax highlighting (optional)
"""

def fibonacci(n):
    """Calculate the nth Fibonacci number using recursion."""
    if n <= 1:
        return n
    return fibonacci(n - 1) + fibonacci(n - 2)

def main():
    # Calculate and print first 10 Fibonacci numbers
    print("First 10 Fibonacci numbers:")
    for i in range(10):
        result = fibonacci(i)
        print(f"F({i}) = {result}")

    # Dictionary example
    config = {
        "name": "Bevy Code Editor",
        "version": "0.1.0",
        "features": ["fast", "customizable", "gpu-accelerated"]
    }

    # List comprehension
    squares = [x**2 for x in range(10)]
    print(f"Squares: {squares}")

    # Try these:
    # - Type to insert text
    # - Backspace/Delete to remove
    # - Arrow keys to navigate
    # - Shift+Arrow to select
    # - Ctrl+A to select all
    # - Ctrl+C to copy, Ctrl+V to paste
    # - Ctrl+Z to undo, Ctrl+Y to redo

if __name__ == "__main__":
    main()
"#;

    set_text_writer.write(bevy_text_editor::SetTextRequested {
        entity,
        text: initial_text.to_string(),
    });
}

fn update_cursor_icon(
    editor_query: Query<(Entity, &TextViewViewport), With<CodeEditor>>,
    input_focus: Res<bevy::input_focus::InputFocus>,
    mut commands: Commands,
    windows: Query<(Entity, &Window), With<Window>>,
) {
    let Ok((editor_entity, viewport)) = editor_query.single() else {
        return;
    };

    if let Ok((window_entity, window)) = windows.single() {
        let Some(cursor_pos) = window.cursor_position() else {
            return;
        };

        // Convert to world coordinates
        let cursor_x = cursor_pos.x - window.width() / 2.0;

        // Calculate code area (from left edge to right before scrollbar)
        let viewport_width = viewport.width as f32;
        let code_area_right = viewport_width / 2.0 - 20.0; // Leave some margin for scrollbar

        // Show text cursor only over code area when focused
        let over_code = cursor_x < code_area_right;
        let is_focused = input_focus.get() == Some(editor_entity);
        let icon = if is_focused && over_code {
            CursorIcon::System(SystemCursorIcon::Text)
        } else {
            CursorIcon::System(SystemCursorIcon::Default)
        };
        commands.entity(window_entity).insert(icon);
    }
}
