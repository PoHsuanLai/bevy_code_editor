# bevscode

Embeddable text editing and rendering plugins for Bevy. Drop them into any app and they coexist with your existing ECS world.

| Crate | What it does |
|---|---|
| **`bevy_instanced_text`** | GPU-instanced text rendering: glyph atlas, soft-wrap layout, overlays. |
| **`bevy_instanced_text_edit`** | Editable text: cursor, selection, edit history, undo/redo, clipboard. |
| **`bevy_tree_sitter`** | Tree-sitter incremental syntax highlighting. |
| **`bevy_lsp`** | Async LSP transport. Responses arrive as Bevy events. |
| **`bevscode`** | Code editor: multi-cursor, folding, brackets, line numbers, LSP UI. |
| **`bevsterm`** | PTY-backed terminal widget. |
| **`bevsmd`** | CommonMark viewer. |

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
