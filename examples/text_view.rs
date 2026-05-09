//! Text View Demo — standalone `InstancedTextPlugins` without any editor.
//!
//! Demonstrates that the engine's `InstancedTextPlugins` (GPU + view systems)
//! can render styled text independently, without `CodeEditorPlugin`, cursor,
//! selection, syntax highlighting, or keybindings.
//!
//! Builds a `DisplayLayout` directly via `trivial_layout`. Mouse-wheel
//! scrolling here is handled by a tiny demo-local system; real consumers
//! that want the editor's scroll/drag/copy behaviour also add
//! `InstancedTextInteractionPlugin`.

use bevy::ecs::message::MessageReader;
use bevy::prelude::*;
use bevy_instanced_text::prelude::*;
use bevy_instanced_text::view::snapshot::{trivial_layout, StyleRun};

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "InstancedTextPlugins Demo — No Editor".to_string(),
            resolution: (800, 600).into(),
            ..default()
        }),
        ..default()
    }).set(bevy::asset::AssetPlugin {
        file_path: "assets".into(),
        ..default()
    }));

    app.add_plugins(InstancedTextPlugins)
        .add_systems(Startup, (setup_camera, setup_text_view))
        .add_systems(Update, handle_scroll)
        .run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn setup_text_view(mut commands: Commands, asset_server: Res<AssetServer>, windows: Query<&Window>) {
    let Ok(window) = windows.single() else {
        return;
    };

    // Bevy introduction content from https://bevy.org/learn/quick-start/introduction/
    let h1 = Color::srgb(1.0, 1.0, 1.0);
    let h2 = Color::srgb(0.9, 0.75, 0.4);
    let body = Color::srgb(0.82, 0.82, 0.82);
    let bullet_key = Color::srgb(0.4, 0.8, 1.0);
    let dim = Color::srgb(0.55, 0.55, 0.55);
    let warn = Color::srgb(1.0, 0.75, 0.3);

    let lines = vec![
        styled_line("Introduction", h1),
        plain_line(""),
        styled_line(
            "If you came here to learn how to make 2D/3D games, visualizations,",
            body,
        ),
        styled_line(
            "user interfaces, or other graphical applications with Bevy,",
            body,
        ),
        styled_line("this is the right place.", body),
        plain_line(""),
        plain_line(""),
        styled_line("What's a BEVY?", h2),
        plain_line(""),
        styled_line("A bevy is a group of birds!", body),
        plain_line(""),
        styled_line(
            "Bevy is also described as \"a refreshingly simple data-driven",
            body,
        ),
        styled_line(
            "game engine built in Rust.\" It is free and open-source under",
            body,
        ),
        styled_line("the MIT or Apache 2.0 licenses.", body),
        plain_line(""),
        plain_line(""),
        styled_line("Design Goals", h2),
        plain_line(""),
        styled_line("Bevy aims to be:", body),
        plain_line(""),
        multi_segment_line(vec![
            ("  Capable     ", bullet_key),
            ("— Complete 2D and 3D feature set", body),
        ]),
        multi_segment_line(vec![
            ("  Simple      ", bullet_key),
            ("— Accessible for newcomers, flexible for advanced users", body),
        ]),
        multi_segment_line(vec![
            ("  Data Focused", bullet_key),
            ("— Entity Component System (ECS) architecture", body),
        ]),
        multi_segment_line(vec![
            ("  Modular     ", bullet_key),
            ("— Use only the components you need", body),
        ]),
        multi_segment_line(vec![
            ("  Fast        ", bullet_key),
            ("— Quick app logic with parallel processing capability", body),
        ]),
        multi_segment_line(vec![
            ("  Productive  ", bullet_key),
            ("— Fast compilation times", body),
        ]),
        plain_line(""),
        plain_line(""),
        styled_line("Development Philosophy", h2),
        plain_line(""),
        styled_line(
            "The engine is \"built in the open by volunteers\" using Rust.",
            body,
        ),
        styled_line(
            "The developers emphasize that games represent millions of hours",
            body,
        ),
        styled_line(
            "of human development effort, yet many developers rely on",
            body,
        ),
        styled_line(
            "closed-source commercial engines that take revenue cuts.",
            body,
        ),
        plain_line(""),
        plain_line(""),
        styled_line("Stability Warning", h2),
        plain_line(""),
        styled_line(
            "Important features remain under development and documentation",
            warn,
        ),
        styled_line(
            "may be limited. Breaking API changes occur approximately once",
            warn,
        ),
        styled_line("every 3 months.", warn),
        plain_line(""),
        styled_line(
            "Migration guides are provided, though migrations are not always",
            body,
        ),
        styled_line("straightforward.", body),
        plain_line(""),
        styled_line(
            "The page recommends Godot Engine for production projects",
            dim,
        ),
        styled_line(
            "requiring stability, noting it offers similar open-source",
            dim,
        ),
        styled_line("benefits with greater feature completeness.", dim),
    ];

    // Plain rope text for hit-testing / copy.
    let mut full_text = String::new();
    for (i, (text, _)) in lines.iter().enumerate() {
        full_text.push_str(text);
        if i < lines.len() - 1 {
            full_text.push('\n');
        }
    }
    let buffer = TextBuffer::with_text(&full_text);
    let scroll = ScrollState::default();
    let metrics = ContentMetrics::default();

    // Build the display layout directly. Match the editor's font defaults so
    // the demo's metrics line up with what `update_text_views` uses.
    let line_height = 24.0;
    let char_width = 10.0;
    let baseline_offset = 18.0 * 0.32;
    let layout = trivial_layout(
        &lines,
        line_height,
        char_width,
        baseline_offset,
        Color::srgb(0.85, 0.85, 0.85),
    );

    let font = FontConfig::from_size(16.0)
        .with_font(asset_server.load("fonts/FiraMono-Regular.ttf"))
        .with_bold_font(asset_server.load("fonts/FiraMono-Medium.ttf"));

    commands.spawn((
        TextView,
        buffer,
        scroll,
        metrics,
        font,
        TextViewViewport {
            width: window.resolution.width() as u32,
            height: window.resolution.height() as u32,
            text_area_left: 16.0,
            text_area_top: 16.0,
            ..default()
        },
        layout,
    ));
}

/// Handle mouse wheel scrolling for the text view
fn handle_scroll(
    mut text_views: Query<&mut ScrollState, With<TextView>>,
    mut mouse_wheel: MessageReader<bevy::input::mouse::MouseWheel>,
) {
    let scroll_speed = 40.0;
    for event in mouse_wheel.read() {
        for mut scroll in text_views.iter_mut() {
            scroll.target_scroll_offset += event.y * scroll_speed;
            scroll.target_scroll_offset = scroll.target_scroll_offset.min(0.0);
        }
    }
}

fn styled_line(text: &str, color: Color) -> (String, Vec<StyleRun>) {
    (
        text.to_string(),
        vec![StyleRun::fg_only(0..text.len(), color)],
    )
}

fn plain_line(text: &str) -> (String, Vec<StyleRun>) {
    (text.to_string(), vec![])
}

fn multi_segment_line(segments: Vec<(&str, Color)>) -> (String, Vec<StyleRun>) {
    let mut text = String::new();
    let mut runs = Vec::with_capacity(segments.len());
    let mut byte_cursor = 0;
    for (t, c) in segments {
        let len = t.len();
        text.push_str(t);
        runs.push(StyleRun::fg_only(byte_cursor..byte_cursor + len, c));
        byte_cursor += len;
    }
    (text, runs)
}
