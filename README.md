# bevscode

[![CI](https://github.com/PoHsuanLai/bevscode/actions/workflows/ci.yml/badge.svg)](https://github.com/PoHsuanLai/bevscode/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)

Embeddable text editing and rendering plugins for Bevy. Drop them into any app and they coexist with your existing ECS world.

![Demo](https://raw.githubusercontent.com/PoHsuanLai/bevscode/main/assets/demo.gif)

**Scope:** this is a component library, not a standalone IDE. It gives you a capable code-editing widget you can embed inside a Bevy application — window management, project trees, debugger UIs, and similar IDE-level concerns are outside its scope.

| Crate | What it does | |
|---|---|---|
| **[`bevy_instanced_text`](crates/bevy_instanced_text)** | GPU-instanced text rendering: glyph atlas, soft-wrap layout, overlays. | [![crates.io](https://img.shields.io/crates/v/bevy_instanced_text.svg)](https://crates.io/crates/bevy_instanced_text) [![docs.rs](https://docs.rs/bevy_instanced_text/badge.svg)](https://docs.rs/bevy_instanced_text) |
| **[`bevy_instanced_text_edit`](crates/bevy_instanced_text_edit)** | Editable text: cursor, selection, edit history, undo/redo, clipboard. | [![crates.io](https://img.shields.io/crates/v/bevy_instanced_text_edit.svg)](https://crates.io/crates/bevy_instanced_text_edit) [![docs.rs](https://docs.rs/bevy_instanced_text_edit/badge.svg)](https://docs.rs/bevy_instanced_text_edit) |
| **[`bevy_tree_sitter`](crates/bevy_tree_sitter)** | Tree-sitter incremental syntax highlighting. | [![crates.io](https://img.shields.io/crates/v/bevy_tree_sitter.svg)](https://crates.io/crates/bevy_tree_sitter) [![docs.rs](https://docs.rs/bevy_tree_sitter/badge.svg)](https://docs.rs/bevy_tree_sitter) |
| **[`bevy_lsp`](crates/bevy_lsp)** | Async LSP transport. Responses arrive as Bevy events. | [![crates.io](https://img.shields.io/crates/v/bevy_lsp.svg)](https://crates.io/crates/bevy_lsp) [![docs.rs](https://docs.rs/bevy_lsp/badge.svg)](https://docs.rs/bevy_lsp) |
| **[`bevscode`](crates/bevscode)** | Code editor: multi-cursor, folding, brackets, line numbers, LSP UI. | [![crates.io](https://img.shields.io/crates/v/bevscode.svg)](https://crates.io/crates/bevscode) [![docs.rs](https://docs.rs/bevscode/badge.svg)](https://docs.rs/bevscode) |
| **[`bevsmd`](crates/bevsmd)** | CommonMark viewer. | [![crates.io](https://img.shields.io/crates/v/bevsmd.svg)](https://crates.io/crates/bevsmd) [![docs.rs](https://docs.rs/bevsmd/badge.svg)](https://docs.rs/bevsmd) |
| **[`bevsterm`](crates/bevsterm)** | PTY-backed terminal widget. | *(not published — wezterm deps not on crates.io)* |

## Bevy compatibility

| bevscode | Bevy |
|---|---|
| 0.1 | 0.18 |

## Quick start

```rust
use bevy::prelude::*;
use bevscode::prelude::*;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, CodeEditorPlugins))
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn((
                CodeEditor,
                TextViewViewport { rect: Rect::new(0.0, 0.0, 800.0, 600.0), ..default() },
            ));
        })
        .run();
}
```

## Composition

Add only what you need:

```rust
// Just GPU text rendering
.add_plugins(InstancedTextPlugins)

// Rendering + interaction
.add_plugins((InstancedTextPlugins, InstancedTextInteractionPlugin))

// Full code editor
.add_plugins(CodeEditorPlugins)
```

State is plain ECS — query it from any system:

```rust
fn status_bar(editors: Query<(&TextBuffer, &CursorState), With<CodeEditor>>) {
    for (buffer, cursor) in &editors { /* … */ }
}
```

## License

MIT OR Apache-2.0
