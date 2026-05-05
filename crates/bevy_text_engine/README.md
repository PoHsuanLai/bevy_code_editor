# bevy_text_engine

GPU-accelerated text rendering for Bevy. Provides primitives — glyph atlas, instanced rendering, soft-wrap layout producer, overlays — for building editors, terminals, chat panels, log viewers, and any other text-heavy UI.

This is the rendering layer. It owns no input model, no UI framework choice, no buffer-edit semantics. Just "given styled text + a viewport, draw it on the GPU."

## What's in the box

- **`TextView`** — marker component for a renderable text view. `#[require]` cascades `TextViewState` (rope + scroll), `TextViewViewport` (rect), `DisplayLayout` (rows of glyphs), `FontConfig`, `TextViewOverlays`, and `Pickable` (for `bevy_picking` integration).
- **`FontConfig`** — per-entity font sizing + optional `Handle<bevy_text::Font>`. Carries `size`, `line_height`, `char_width`. Same handle works in `bevy_text::Text2d` and `TextView`.
- **`DisplayLayout`** — the renderer's input. A list of `ShapedLine`s (text + style runs + per-row line height + padding + indent) plus global metrics. Producers write it; the renderer reads it.
- **Layout producer** — `produce_layouts` system queries entities with `LineFilter` / `LineStyleSource` / `LayoutWrap` Components and writes `DisplayLayout` automatically. Handles soft-wrap with whitespace-aware breaks, fold-aware visibility (via the filter Component), per-row styling (via the styling Component).
- **`trivial_layout` / `trivial_layout_blocks`** — for static content. `trivial_layout` is one row per line; `trivial_layout_blocks` accepts per-row line-height + padding + indent for markdown-style block layout.
- **GPU pipeline** — `GlyphAtlasPlugin` (manages the cosmic-text font system + a 2048×2048 R8 atlas with shelf packing) and `InstancedTextRenderPlugin` (one instanced draw per text view, `GlyphInstance` per glyph).
- **Overlays** — `RectOverlay` rows (cursor caret, selection rectangles, line highlights, find-matches) layered into the same draw call via z-order.

## What's NOT in the box

- No selection model, multi-cursor, undo/redo. (See [`bevy_text_interaction`](../bevy_text_interaction) for pointer interaction; the editor crate has the rest.)
- No syntax highlighting. The engine takes pre-computed `StyleRun`s. (See [`bevy_tree_sitter`](../bevy_tree_sitter) for tree-sitter integration.)
- No `bevy_ui::Node` integration. `TextView` renders to a world-space transform inside a `TextViewViewport` rect; embedding inside a flexbox tree requires writing the rect from `ComputedNode` yourself.

## Quick start

```rust
use bevy::prelude::*;
use bevy_text_engine::prelude::*;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, TextEnginePlugins))
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.spawn((
        TextView,
        FontConfig::from_size(16.0),
        // Provide a DisplayLayout some other way — see below.
    ));
}
```

For static content (no editor, no markdown, just a paragraph of text), populate the `DisplayLayout` with `trivial_layout`:

```rust
use bevy_text_engine::view::snapshot::trivial_layout;

let layout = trivial_layout(
    &[
        ("Hello, world!".to_string(), vec![]),
        ("This is a second line.".to_string(), vec![]),
    ],
    20.0,    // line_height
    8.0,     // char_width
    5.0,     // baseline_offset
    Color::WHITE,
);
commands.entity(my_view).insert(layout);
```

For markdown-style layout with mixed line heights, padding, soft-wrap, and
block-level decoration (background fills + borders for code blocks /
blockquotes / chat-message bubbles):

```rust
use bevy_text_engine::view::snapshot::{trivial_layout_blocks, TrivialBlock};

let blocks = vec![
    TrivialBlock::new("# Heading")
        .with_line_height(28.0)
        .with_padding(12.0, 6.0)
        .with_wrap_chars(0),                          // headings don't wrap
    TrivialBlock::new("Lorem ipsum dolor sit amet, consectetur adipiscing elit."),
    TrivialBlock::new("fn main() { println!(\"hi\"); }")
        .with_padding(8.0, 8.0)
        .with_block_background(Color::srgb(0.12, 0.12, 0.14))
        .with_block_corner_radius(4.0),
    TrivialBlock::new("> a quoted line")
        .with_padding(4.0, 4.0)
        .with_block_border(Color::srgb(0.5, 0.5, 0.5), 1.0),
];
// 6th arg = default wrap budget in characters (None = no wrap).
let layout = trivial_layout_blocks(&blocks, 16.0, 8.0, 5.0, Color::WHITE, Some(60));
```

`with_block_background` paints a filled quad spanning the block's full
vertical extent (padding_top + all wrap rows + padding_bottom), distinct
from per-row `line_bg`. `with_block_border(color, width)` adds a uniform
border drawn from four edge quads. Blocks with no decoration cost zero.

For dynamic content (an editor, a streaming log viewer), drive the producer via `LineFilter` / `LineStyleSource` Components — see the editor crate for a worked example.

## Plugin composition

`TextEnginePlugins` is a `PluginGroup` bundling:

- `GlyphAtlasPlugin` — atlas resource bootstrap.
- `InstancedTextRenderPlugin` — instanced draw pipeline.
- `TextEnginePlugin` — view systems (`produce_layouts`, `update_text_views`, `prewarm_atlas_for_layout`, `animate_text_view_scroll`).

Mirror of `bevy::DefaultPlugins`. Hosts that want fine-grained control can add only the constituents they need.

## System sets

- **`TextViewRenderSet`** — the rendering system runs in this set. Downstream systems can `.before/.after(TextViewRenderSet)`.

## Cargo features

The engine has no own feature flags; all behavior is always-on. Bevy features pulled in: `bevy_render`, `bevy_core_pipeline`, `bevy_asset`, `bevy_sprite`, `bevy_color`, `bevy_mesh`, `bevy_camera`, `bevy_log`, `bevy_picking`, `bevy_text` (for the `Font` asset).

## License

MIT OR Apache-2.0
