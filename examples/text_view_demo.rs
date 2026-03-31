//! Text View Demo — standalone TextViewPlugin without any editor
//!
//! Demonstrates that TextViewPlugin can render styled text independently,
//! without CodeEditorPlugin, cursor, selection, syntax highlighting, or keybindings.
//!
//! This is the foundation for building chat panels, log viewers, etc.

use bevy::ecs::message::MessageReader;
use bevy::prelude::*;
use bevy_code_editor::settings::EditorSettingsBuilder;
use bevy_code_editor::text_view::*;
use bevy_code_editor::types::LineSegment;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "TextViewPlugin Demo — No Editor".to_string(),
            resolution: (800, 600).into(),
            ..default()
        }),
        ..default()
    }));

    // Insert minimal settings (font, theme, performance) needed by TextViewPlugin
    EditorSettingsBuilder::default().build().insert_into(&mut app);

    app.add_plugins(TextViewPlugin)
        .add_systems(Startup, (setup_camera, setup_text_view))
        .add_systems(Update, handle_scroll)
        .run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d::default());
}

fn setup_text_view(mut commands: Commands, windows: Query<&Window>) {
    let Ok(window) = windows.single() else {
        return;
    };

    // Create styled text content — simulating a chat conversation
    let mut state = TextViewState::with_text("");

    // Build chat-like content with styled lines
    let lines = vec![
        // User message
        styled_line("You:", Color::srgb(0.4, 0.7, 1.0)),
        styled_line(
            "  Can you explain how GPU instanced rendering works?",
            Color::srgb(0.9, 0.9, 0.9),
        ),
        plain_line(""),
        // Assistant message
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
        styled_line(
            "  data buffers.",
            Color::srgb(0.85, 0.85, 0.85),
        ),
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
        // Code block with different colors
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
        // More conversation
        styled_line("You:", Color::srgb(0.4, 0.7, 1.0)),
        styled_line("  That makes sense! How does the atlas work?", Color::srgb(0.9, 0.9, 0.9)),
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

    // Build the rope text and styled lines
    let mut full_text = String::new();
    for (i, (text, _)) in lines.iter().enumerate() {
        full_text.push_str(text);
        if i < lines.len() - 1 {
            full_text.push('\n');
        }
    }

    state.set_text(&full_text);

    // Set styled lines
    for (i, (_, segments)) in lines.iter().enumerate() {
        if !segments.is_empty() {
            state.set_styled_line(i, segments.clone());
        }
    }

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
            // Clamp scroll to not go below content
            state.target_scroll_offset = state.target_scroll_offset.min(0.0);
            state.needs_scroll_update = true;
        }
    }
}

// Helper to create a single-color styled line
fn styled_line(text: &str, color: Color) -> (String, Vec<LineSegment>) {
    (
        text.to_string(),
        vec![LineSegment {
            text: text.to_string(),
            color,
            background: None,
        }],
    )
}

// Helper to create a plain (unstyled) line
fn plain_line(text: &str) -> (String, Vec<LineSegment>) {
    (text.to_string(), vec![])
}

// Helper to create a multi-segment styled line
fn multi_segment_line(segments: Vec<(&str, Color)>) -> (String, Vec<LineSegment>) {
    let text: String = segments.iter().map(|(t, _)| *t).collect();
    let line_segments = segments
        .into_iter()
        .map(|(t, c)| LineSegment {
            text: t.to_string(),
            color: c,
            background: None,
        })
        .collect();
    (text, line_segments)
}
