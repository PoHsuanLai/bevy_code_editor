# bevsmd

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

## License

MIT OR Apache-2.0
