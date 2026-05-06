//! Side-by-side: a `CodeEditor` on the left, a `BevyTerminal` on the
//! right. Both share the engine's atlas + render pipeline; click into
//! either to focus it.
//!
//! Run with: `cargo run -p bevy_terminal --example editor_and_terminal`

use bevy::prelude::*;
use bevy_code_editor::prelude::*;
use bevy_terminal::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "bevy_terminal — editor + terminal".into(),
                resolution: [1280u32, 720u32].into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(CodeEditorPlugin)
        .add_plugins(BevyTerminalPlugin)
        .add_systems(Startup, (setup_camera, layout_panes))
        .run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn layout_panes(
    asset_server: Res<AssetServer>,
    windows: Query<&Window>,
    mut editors: Query<&mut TextViewViewport, (With<CodeEditor>, Without<BevyTerminal>)>,
    mut commands: Commands,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let logical_w = window.width() as u32;
    let logical_h = window.height() as u32;
    let half_w = logical_w / 2;

    // Editor occupies the left half.
    if let Ok(mut viewport) = editors.single_mut() {
        viewport.width = half_w;
        viewport.height = logical_h;
        viewport.hit_test_position = Vec2::ZERO;
    }

    // Terminal entity for the right half.
    let regular: Handle<bevy::text::Font> = asset_server.load("fonts/FiraMono-Regular.ttf");
    let bold: Handle<bevy::text::Font> = asset_server.load("fonts/FiraMono-Medium.ttf");
    let font = FontConfig::from_size(14.0)
        .with_font(regular)
        .with_bold_font(bold);

    commands.spawn((
        BevyTerminal,
        font,
        TextViewViewport {
            width: logical_w - half_w,
            height: logical_h,
            text_area_left: 12.0,
            text_area_top: 12.0,
            // Anchor the terminal's hit-test area to the right half so
            // pointer events route correctly even though both views
            // render through the same Camera2d.
            hit_test_position: Vec2::new(half_w as f32, 0.0),
            ..default()
        },
    ));
}
