# bevy_code_editor

A GPU-accelerated text rendering engine for Bevy, plus a code editor built on top of it.

This started as a code editor and grew into the underlying primitives. The workspace is now split into a hermetic engine plus several peer crates a host can compose:

| Crate | What it does | Depends on |
|---|---|---|
| **`bevy_text_engine`** | GPU glyph atlas, instanced rendering, soft-wrap layout producer, overlays | bevy + cosmic-text + swash |
| **`bevy_text_interaction`** | Pointer + focused-keyboard interaction (scroll, drag-select, copy) for `TextView` entities | `bevy_text_engine` + bevy_picking + bevy_input_focus |
| **`bevy_tree_sitter`** | Tree-sitter parser + incremental highlights, text-rendering-agnostic | bevy + tree-sitter |
| **`bevy_lsp`** | Async LSP transport (async-lsp on a shared tokio runtime), per-document Components, position helpers | bevy + async-lsp + lsp-types |
| **`bevy_code_editor`** | The code-editor consumer: cursor, selection, multi-cursor, syntax adapter, LSP UI, scrollbar, line numbers | all of the above |

## What you get

If you want **a code editor in your Bevy app**:

```rust
use bevy::prelude::*;
use bevy_code_editor::prelude::*;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, CodeEditorPlugin::standalone()))
        .run();
}
```

`CodeEditorPlugin::standalone()` is a `PluginGroup` that bundles the engine, interaction, the editor, and a default UI plugin (line numbers, separator, camera). One line of code, working editor.

If you want **a chat box, log viewer, terminal, or anything that just renders styled text**:

```rust
use bevy::prelude::*;
use bevy_text_engine::prelude::*;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, TextEnginePlugins))
        .add_systems(Startup, spawn_panel)
        .run();
}

fn spawn_panel(mut commands: Commands) {
    commands.spawn((
        TextView,
        FontConfig::from_size(16.0),
        // your data layer writes DisplayLayout each frame
    ));
}
```

The engine's job is "given a `DisplayLayout`, render it." How you produce the layout is up to you. For static content, see `view::trivial_layout` / `view::trivial_layout_blocks`.

## Examples

```bash
# Basic editor
cargo run --example basic

# Two editors in one window, independent state, separate cameras
cargo run --example multi_editor

# Tree-sitter syntax highlighting (Rust)
cargo run --example tree-sitter

# LSP integration (rust-analyzer)
cargo run --example lsp --features lsp

# Engine-only demo, no editor — proves TextEnginePlugins works alone
cargo run --example text_view_demo
```

## Composition

The plugins are explicit and additive — no auto-add of unrelated machinery. Mix and match:

```rust
// Just the rendering engine
.add_plugins(TextEnginePlugins)

// Engine + pointer/keyboard interaction
.add_plugins((TextEnginePlugins, TextInteractionPlugin))

// Engine + interaction + editor (build it up explicitly)
.add_plugins((TextEnginePlugins, TextInteractionPlugin, CodeEditorPlugin))

// All of the above plus default UI + camera (one-line)
.add_plugins(CodeEditorPlugin::standalone())
```

LSP and tree-sitter are gated by feature flags on `bevy_code_editor`:

- `tree-sitter` (default) — pulls in `bevy_tree_sitter` + a syntax-highlighting adapter.
- `lsp` — pulls in `bevy_lsp` + the editor's LSP UI.

## Embedding multiple editors

The whole workspace is per-entity. `CodeEditor::default()` can be spawned multiple times; `bevy_picking` routes clicks to whichever is hovered, `bevy_input_focus` routes keyboard to whichever is focused. Each editor has its own `FontConfig`, scroll state, fold state, LSP client, syntax tree.

See `examples/multi_editor.rs`.

## Sharing fonts with bevy_text

`FontConfig` carries `Option<Handle<bevy_text::Font>>`:

```rust
let font: Handle<bevy::text::Font> = asset_server.load("fonts/JetBrainsMono.ttf");

// In bevy_text:
commands.spawn((Text2d::new("hello"), TextFont { font: font.clone(), ..default() }));

// In bevy_text_engine:
commands.spawn((TextView, FontConfig::from_size(16.0).with_font(font)));
```

Same handle, single asset load, asset hot-reload works for both.

## Feature flags

`bevy_code_editor` features:

- `tree-sitter` (default) — syntax highlighting via `bevy_tree_sitter`
- `lsp` — language server integration via `bevy_lsp`

Minimal build (no syntax highlighting, no LSP):

```bash
cargo build -p bevy_code_editor --no-default-features
```

## License

MIT OR Apache-2.0

## Credits

Built with:

- [Bevy](https://bevyengine.org/) — game engine + ECS
- [cosmic-text](https://github.com/pop-os/cosmic-text) — text shaping
- [swash](https://github.com/dfrg/swash) — glyph rasterization
- [ropey](https://github.com/cessen/ropey) — rope-based text buffer
- [tree-sitter](https://tree-sitter.github.io/) — incremental parsing
- [async-lsp](https://github.com/oxalica/async-lsp) — LSP transport
- [lsp-types](https://github.com/gluon-lang/lsp-types) — LSP protocol types
- [leafwing-input-manager](https://github.com/Leafwing-Studios/leafwing-input-manager) — action mapping
- [bevy-tokio-tasks](https://github.com/EkardNT/bevy-tokio-tasks) — shared tokio runtime as a Bevy resource

Inspired by [Zed](https://zed.dev/), [Helix](https://helix-editor.com/), and VSCode.
