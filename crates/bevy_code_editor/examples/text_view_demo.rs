//! Text View Demo — standalone `TextEnginePlugins` without any editor
//!
//! Demonstrates that the engine's `TextEnginePlugins` (GPU + view systems)
//! can render styled text independently, without `CodeEditorPlugin`, cursor,
//! selection, syntax highlighting, or keybindings.
//!
//! As of step 7 the demo builds a `DisplayLayout` directly via `trivial_layout`
//! rather than going through `TextViewState.styled_lines`. Mouse-wheel
//! scrolling here is handled by a tiny demo-local system; real consumers
//! that want the editor's scroll/drag/copy behaviour also add
//! `TextInteractionPlugin`.

use bevy::ecs::message::MessageReader;
use bevy::prelude::*;
use bevy_text_engine::prelude::*;
use bevy_text_engine::view::snapshot::{trivial_layout, StyleRun};

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "TextEnginePlugins Demo — No Editor".to_string(),
            resolution: (800, 600).into(),
            ..default()
        }),
        ..default()
    }));

    app.add_plugins(TextEnginePlugins)
        .add_systems(Startup, (setup_camera, setup_text_view))
        .add_systems(Update, handle_scroll)
        .run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn setup_text_view(mut commands: Commands, windows: Query<&Window>) {
    let Ok(window) = windows.single() else {
        return;
    };

    let lines = vec![
        styled_line("You:", Color::srgb(0.4, 0.7, 1.0)),
        styled_line(
            "  Can you explain how GPU instanced rendering works?",
            Color::srgb(0.9, 0.9, 0.9),
        ),
        plain_line(""),
        styled_line("Assistant:", Color::srgb(0.5, 1.0, 0.5)),
        styled_line(
            "  GPU instanced rendering draws many copies of geometry",
            Color::srgb(0.85, 0.85, 0.85),
        ),
        styled_line(
            "  in a single draw call. Each instance gets its own",
            Color::srgb(0.85, 0.85, 0.85),
        ),
        styled_line(
            "  position, color, and UV coordinates via per-instance",
            Color::srgb(0.85, 0.85, 0.85),
        ),
        styled_line("  data buffers.", Color::srgb(0.85, 0.85, 0.85)),
        plain_line(""),
        styled_line(
            "  For text rendering, each glyph is an instance of a",
            Color::srgb(0.85, 0.85, 0.85),
        ),
        styled_line(
            "  textured quad. The shader samples from a glyph atlas",
            Color::srgb(0.85, 0.85, 0.85),
        ),
        styled_line(
            "  texture to render the correct character.",
            Color::srgb(0.85, 0.85, 0.85),
        ),
        plain_line(""),
        styled_line("  ```rust", Color::srgb(0.6, 0.6, 0.6)),
        multi_segment_line(vec![
            ("  fn ", Color::srgb(0.8, 0.5, 0.8)),
            ("render_glyphs", Color::srgb(0.9, 0.8, 0.4)),
            ("(instances: &[", Color::srgb(0.9, 0.9, 0.9)),
            ("GlyphInstance", Color::srgb(0.4, 0.8, 0.8)),
            ("]) {", Color::srgb(0.9, 0.9, 0.9)),
        ]),
        multi_segment_line(vec![
            ("      gpu", Color::srgb(0.9, 0.9, 0.9)),
            (".", Color::srgb(0.9, 0.9, 0.9)),
            ("draw_instanced", Color::srgb(0.9, 0.8, 0.4)),
            ("(instances);", Color::srgb(0.9, 0.9, 0.9)),
        ]),
        styled_line("  }", Color::srgb(0.9, 0.9, 0.9)),
        styled_line("  ```", Color::srgb(0.6, 0.6, 0.6)),
        plain_line(""),
        styled_line("You:", Color::srgb(0.4, 0.7, 1.0)),
        styled_line(
            "  That makes sense! How does the atlas work?",
            Color::srgb(0.9, 0.9, 0.9),
        ),
        plain_line(""),
        styled_line("Assistant:", Color::srgb(0.5, 1.0, 0.5)),
        styled_line(
            "  The glyph atlas is a large texture that stores all",
            Color::srgb(0.85, 0.85, 0.85),
        ),
        styled_line(
            "  rasterized characters. When a new character is needed,",
            Color::srgb(0.85, 0.85, 0.85),
        ),
        styled_line(
            "  it's rasterized once and packed into the atlas. Future",
            Color::srgb(0.85, 0.85, 0.85),
        ),
        styled_line(
            "  uses just reference the UV coordinates in the atlas.",
            Color::srgb(0.85, 0.85, 0.85),
        ),
    ];

    // Plain rope text for hit-testing / copy.
    let mut full_text = String::new();
    for (i, (text, _)) in lines.iter().enumerate() {
        full_text.push_str(text);
        if i < lines.len() - 1 {
            full_text.push('\n');
        }
    }
    let state = TextViewState::with_text(&full_text);

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

    commands.spawn((
        TextView,
        state,
        TextViewViewport {
            width: window.physical_width(),
            height: window.physical_height(),
            text_area_left: 16.0,
            text_area_top: 16.0,
            ..default()
        },
        layout,
    ));
}

/// Handle mouse wheel scrolling for the text view
fn handle_scroll(
    mut text_views: Query<&mut TextViewState, With<TextView>>,
    mut mouse_wheel: MessageReader<bevy::input::mouse::MouseWheel>,
) {
    let scroll_speed = 40.0;
    for event in mouse_wheel.read() {
        for mut state in text_views.iter_mut() {
            state.target_scroll_offset += event.y * scroll_speed;
            state.target_scroll_offset = state.target_scroll_offset.min(0.0);
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
