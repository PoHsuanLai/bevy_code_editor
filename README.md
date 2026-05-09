# bevy_code_editor

Embeddable text editing and rendering plugins for Bevy. Drop them into any app — a game, a dev tool, a creative environment — and they coexist with your existing ECS world.

Each crate is independent. Use only what you need:

| Crate | What it does |
|---|---|
| **`bevy_instanced_text`** | GPU-instanced text rendering: glyph atlas, soft-wrap layout, overlays. Takes a `DisplayLayout` component and draws it. |
| **`bevy_instanced_text_edit`** | Editable text: cursor, selection, edit history, undo/redo, clipboard. State lives in queryable components on each entity. |
| **`bevy_tree_sitter`** | Tree-sitter parser + incremental syntax tokens. Writes highlight data as components; you decide what to do with them. |
| **`bevy_lsp`** | Async LSP transport. Responses arrive as Bevy events; per-document state is queryable. |
| **`bevscode`** | Code editor: multi-cursor, folding, brackets, line numbers, LSP UI, syntax adapter. Embeds into any Bevy app as a spawnable entity. |
| **`bevsterm`** | PTY-backed terminal widget. Embeds into any Bevy app as a spawnable entity. |
| **`bevsmd`** | CommonMark viewer. Spawnable entity, scrollable and selectable. |

## Embedding a code editor

Spawn `CodeEditor` anywhere in your world — inside a game, next to a 3D scene, in a split-pane UI:

```rust
use bevy::prelude::*;
use bevscode::prelude::*;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, CodeEditorPlugins))
        .add_systems(Startup, spawn_editor)
        .run();
}

fn spawn_editor(mut commands: Commands) {
    commands.spawn((
        CodeEditor,
        TextViewViewport { rect: Rect::new(0.0, 0.0, 800.0, 600.0), ..default() },
    ));
}
```

Multiple editors, independent state:

```rust
fn spawn_two(mut commands: Commands) {
    commands.spawn((CodeEditor, TextViewViewport { rect: Rect::new(0.0, 0.0, 800.0, 600.0), ..default() }));
    commands.spawn((CodeEditor, TextViewViewport { rect: Rect::new(800.0, 0.0, 1600.0, 600.0), ..default() }));
}
```

`bevy_picking` routes mouse to whichever editor is hovered; `bevy_input_focus` routes keyboard to whichever was clicked last.

## Reading state from your systems

All editor state is plain ECS — query it from any system, no special API:

```rust
fn my_status_bar(
    editors: Query<(&TextBuffer, &CursorState, &EditorSyntaxState), With<CodeEditor>>,
) {
    for (buffer, cursor, syntax) in &editors {
        // current line count, cursor position, active language — all just components
    }
}
```

The same applies to `bevsterm` (query `TerminalGridSnapshot`, `TerminalShellInfo`) and `bevsmd` (query `MarkdownLinks`). The plugins write data; your systems read it however fits your app.

## Composition

Add only what your app needs:

```rust
// Just GPU text rendering
.add_plugins(InstancedTextPlugins)

// Rendering + pointer/keyboard interaction
.add_plugins((InstancedTextPlugins, InstancedTextInteractionPlugin))

// Full code editor
.add_plugins(CodeEditorPlugins)
```

Disable sub-plugins your app already provides:

```rust
CodeEditorPlugins.build().disable::<EditorUiPlugin>()
```

## Feature flags

`bevscode`:
- `tree-sitter` (default) — syntax highlighting via `bevy_tree_sitter`
- `lsp` — language server integration via `bevy_lsp`

Minimal build:
```bash
cargo build -p bevscode --no-default-features
```

## Examples

```bash
cargo run --example basic
cargo run --example multi_editor
cargo run --example tree-sitter
cargo run --example lsp --features lsp
cargo run --example text_view_demo   # rendering only, no editor
```

## License

MIT OR Apache-2.0

## Credits

- [Bevy](https://bevyengine.org/) — game engine + ECS
- [cosmic-text](https://github.com/pop-os/cosmic-text) — text shaping
- [swash](https://github.com/dfrg/swash) — glyph rasterization
- [ropey](https://github.com/cessen/ropey) — rope-based text buffer
- [tree-sitter](https://tree-sitter.github.io/) — incremental parsing
- [async-lsp](https://github.com/oxalica/async-lsp) — LSP transport
- [leafwing-input-manager](https://github.com/Leafwing-Studios/leafwing-input-manager) — action mapping

Inspired by [Zed](https://zed.dev/), [Helix](https://helix-editor.com/), and VSCode.
