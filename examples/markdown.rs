//! Basic markdown viewer demo. Renders a sample document covering all
//! supported features: headings, bold/italic, inline code, fenced code
//! blocks, lists, blockquotes, links, and a horizontal rule.

use bevy::ecs::message::MessageReader;
use bevy::prelude::*;
use bevsmd::prelude::*;

const SCROLL_SPEED: f32 = 40.0;

const SAMPLE: &str = "\
# Markdown Viewer

A *markdown* viewer for Bevy. Renders rich text with real bold and italic
faces (or synthesizes them when no matching face is loaded).

## Features

- **Bold** and *italic*, plus ***both at once***
- Inline `code` chips with a tinted background
- ~~Strikethrough~~ via GFM
- [External links](https://github.com/PoHsuanLai/bevscode) in a \
  distinct color
- Soft-wrapping at the viewport width

### Code blocks

```rust
fn greet(name: &str) {
    println!(\"Hello, {name}!\");
}
```

### Blockquote

> A quoted paragraph. The renderer applies a thin border so the block \
> visually separates from surrounding body text.

---

That's the whole demo.\n";

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "bevsmd — basic demo".to_string(),
                resolution: (900, 700).into(),
                ..default()
            }),
            ..default()
        }).set(bevy::asset::AssetPlugin {
            file_path: "assets".into(),
            ..default()
        }))
        .add_plugins(MarkdownViewerPlugins)
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
    let Ok(window) = windows.single() else { return };

    let font = MarkdownFont::from_size(16.0)
        .with_font(asset_server.load("fonts/FiraMono-Regular.ttf"))
        .with_bold_font(asset_server.load("fonts/FiraMono-Medium.ttf"));

    let code_font = asset_server.load("fonts/CourierNew-Regular.ttf");

    commands.spawn((
        MarkdownViewerBundle {
            doc: MarkdownDoc::new(SAMPLE),
            viewport: MarkdownViewport {
                width: window.width() as u32,
                height: window.height() as u32,
                text_area_left: 24.0,
                text_area_top: 24.0,
                ..default()
            },
            font,
            ..default()
        },
        MarkdownCodeFont(code_font),
    ));
}

fn handle_scroll(
    mut scroll_state: Query<(&mut ScrollState, &MarkdownViewport, &DisplayLayout)>,
    mut mouse_wheel: MessageReader<bevy::input::mouse::MouseWheel>,
) {
    for event in mouse_wheel.read() {
        for (mut scroll, viewport, layout) in scroll_state.iter_mut() {
            scroll.target_scroll_offset += event.y * SCROLL_SPEED;
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
