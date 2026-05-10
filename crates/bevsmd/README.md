# bevsmd

[![crates.io](https://img.shields.io/crates/v/bevsmd.svg)](https://crates.io/crates/bevsmd)
[![docs.rs](https://docs.rs/bevsmd/badge.svg)](https://docs.rs/bevsmd)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/PoHsuanLai/bevscode/blob/main/LICENSE-MIT)
[![Bevy](https://img.shields.io/badge/Bevy-0.18-blue)](https://bevyengine.org)

Read-only CommonMark markdown viewer for Bevy. Parses markdown with `pulldown-cmark` and renders rich text (headings, bold, italic, inline code, fenced code blocks, lists, links) via `bevy_instanced_text`.

## Quick start

```rust
use bevy::prelude::*;
use bevsmd::prelude::*;
use bevy_instanced_text::InstancedTextPlugins;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, InstancedTextPlugins, MarkdownViewerPlugin))
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(MarkdownViewer::new("# Hello\n\nSome **bold** text."));
        })
        .run();
}
```

## Features

- CommonMark headings, paragraphs, bold, italic, inline code, fenced code blocks, ordered and unordered lists, links
- Optional syntax highlighting for fenced code blocks via the `tree-sitter` feature

## Bevy compatibility

| `bevsmd` | Bevy |
|---|---|
| 0.1 | 0.18 |

## License

MIT OR Apache-2.0
