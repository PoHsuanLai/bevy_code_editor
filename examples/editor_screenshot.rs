//! Auto-screenshot debug example.
//!
//! Same content as `editor_basic` but takes a PNG of the primary window
//! after a few frames, then exits. Used to iterate on the rendering
//! pipeline without a human in the loop.

use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy_code_editor::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Bevy Code Editor (Screenshot)".to_string(),
                resolution: (1400, 900).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(CodeEditorPlugins)
        .add_systems(Startup, setup_camera)
        .add_systems(PostStartup, setup_editor)
        .add_systems(Update, capture_after_warmup)
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
    print("First 10 Fibonacci numbers:")
    for i in range(10):
        result = fibonacci(i)
        print(f"F({i}) = {result}")

if __name__ == "__main__":
    main()
"#;
    set_text_writer.write(bevy_text_editor::SetTextRequested {
        entity,
        text: initial_text.to_string(),
    });
}

fn capture_after_warmup(
    mut commands: Commands,
    mut frame: Local<u32>,
    mut done: Local<bool>,
    mut exit: MessageWriter<AppExit>,
) {
    if *done {
        // After taking the screenshot, give one more frame for the save
        // task to complete, then exit.
        *frame += 1;
        if *frame > 60 {
            exit.write(AppExit::Success);
        }
        return;
    }
    *frame += 1;
    // Wait long enough for the editor to spawn entities and render at
    // least one frame with text.
    if *frame == 90 {
        let path = "/tmp/editor_screenshot.png";
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
        info!("[screenshot] requested capture to {}", path);
        *done = true;
        *frame = 0;
    }
}
