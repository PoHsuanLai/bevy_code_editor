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
                title: "Bevy Code Editor - GPU Text Rendering".to_string(),
                resolution: (1400, 900).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(CodeEditorPlugin::default().with_render_mode(RenderMode::GpuText))
        .add_systems(Startup, setup_editor)
        .add_systems(Update, update_cursor_icon)
        .run();
}

fn setup_editor(mut state: ResMut<CodeEditorState>) {
    // Always focused in basic editor (no UI competing for input)
    state.is_focused = true;

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

    state.set_text(initial_text);
}

fn update_cursor_icon(
    state: Res<CodeEditorState>,
    mut commands: Commands,
    windows: Query<Entity, With<Window>>,
) {
    if let Ok(window_entity) = windows.single() {
        // Always show text cursor since editor is always focused in this example
        let icon = if state.is_focused {
            CursorIcon::System(SystemCursorIcon::Text)
        } else {
            CursorIcon::System(SystemCursorIcon::Default)
        };
        commands.entity(window_entity).insert(icon);
    }
}
