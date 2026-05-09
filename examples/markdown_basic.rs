//! Basic markdown viewer demo. Renders a sample document covering all
//! supported features: headings, bold/italic, inline code, fenced code
//! blocks, lists, blockquotes, links, and a horizontal rule.
//!
//! Drop a bold/italic font face into `assets/fonts/` to see the renderer
//! pick the real face — without one, the engine synthesizes via stroke
//! doubling (bold) and skew (italic).

use bevy::ecs::message::MessageReader;
use bevy::prelude::*;
use bevy_markdown::{MarkdownDoc, MarkdownViewerPlugin};
use bevy_instanced_text::{
    ContentMetrics, DisplayLayout, FontConfig, FontSynthesis, ScrollState, TextBuffer,
    InstancedTextPlugins, TextView, TextViewViewport,
};

const SCROLL_SPEED: f32 = 40.0;

const SAMPLE: &str = "\
# Markdown Viewer

A *markdown* viewer for Bevy. Built on **`bevy_instanced_text`**, it renders \
rich text with real bold and italic faces (or synthesizes them when no \
matching face is loaded).

## Features

- **Bold** and *italic*, plus ***both at once***
- Inline `code` chips with a tinted background
- ~~Strikethrough~~ via GFM
- [External links](https://github.com/PoHsuanLai/bevy_code_editor) in a \
  distinct color
- Soft-wrapping at the viewport width

### Code blocks

```rust
fn render(layout: &DisplayLayout) -> Vec<GlyphInstance> {
    layout.lines.iter().flat_map(emit).collect()
}
```

### Blockquote

> A quoted paragraph. The renderer applies a thin border so the block \
> visually separates from surrounding body text. Selection still hits \
> the rendered text underneath.

---

That's the whole demo.\n";

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "bevy_markdown — basic demo".to_string(),
            resolution: (900, 700).into(),
            ..default()
        }),
        ..default()
    }))
    .add_plugins(InstancedTextPlugins)
    .add_plugins(MarkdownViewerPlugin)
    .add_systems(Startup, (setup_camera, setup_viewer))
    .add_systems(Update, handle_scroll)
    .run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn setup_viewer(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    windows: Query<&Window>,
) {
    let Ok(window) = windows.single() else {
        return;
    };

    let regular: Handle<bevy::text::Font> = asset_server.load("fonts/FiraMono-Regular.ttf");
    let medium: Handle<bevy::text::Font> = asset_server.load("fonts/FiraMono-Medium.ttf");

    // Use the Medium face as a stand-in "bold" if a real Bold isn't
    // bundled. The renderer treats `font_weight >= 600` as bold; with
    // no bold slot loaded, synthesis kicks in (faux bold via stroke
    // doubling). With a Medium loaded as bold, the actual face renders.
    let font = FontConfig::from_size(16.0)
        .with_font(regular)
        .with_bold_font(medium)
        .with_font_synthesis(FontSynthesis::default());

    // `Camera2d` operates in logical pixels by default, so the viewport
    // dimensions need to match the window's logical size — not the
    // physical size. Using physical pixels here doubles the wrap
    // budget on retina displays and makes long lines run off-screen.
    let logical_w = window.width() as u32;
    let logical_h = window.height() as u32;

    commands.spawn((
        TextView,
        TextBuffer::default(),
        ScrollState::default(),
        ContentMetrics::default(),
        TextViewViewport {
            width: logical_w,
            height: logical_h,
            text_area_left: 24.0,
            text_area_top: 24.0,
            ..default()
        },
        font,
        MarkdownDoc::new(SAMPLE),
        DisplayLayout::default(),
    ));
}

/// Mouse-wheel scroll, clamped to the laid-out content height.
///
/// `target_scroll_offset` is negative going down (matches the editor's
/// convention). The lower bound is `-(content_height - viewport_height)`
/// so the last row stays visible at maximum scroll; the upper bound is 0
/// so the first row stays at the top.
fn handle_scroll(
    mut text_views: Query<(&mut ScrollState, &TextViewViewport, &DisplayLayout), With<TextView>>,
    mut mouse_wheel: MessageReader<bevy::input::mouse::MouseWheel>,
) {
    for event in mouse_wheel.read() {
        for (mut scroll, viewport, layout) in text_views.iter_mut() {
            scroll.target_scroll_offset += event.y * SCROLL_SPEED;
            // Content height = (last row's bottom) - (first row's top),
            // computed from the live layout's shifted y_top values. The
            // shift cancels out so this is invariant to scroll.
            let content_h = match (layout.lines.first(), layout.lines.last()) {
                (Some(first), Some(last)) => {
                    last.y_top + last.line_height.unwrap_or(layout.line_height) - first.y_top
                }
                _ => 0.0,
            };
            let max_scroll = (content_h - viewport.height as f32).max(0.0);
            scroll.target_scroll_offset = scroll.target_scroll_offset.clamp(-max_scroll, 0.0);
        }
    }
}
