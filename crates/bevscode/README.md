# bevscode

Embeddable code editor for Bevy. Spawn `CodeEditor` into any app and it runs as a normal ECS entity.

**Scope:** `bevscode` is a widget, not a standalone IDE. Window management, project trees, debugger UIs, and similar IDE-level concerns are left to the host application.

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

## Features

Multi-cursor, folding, bracket matching, line numbers, scrollbar, syntax highlighting (via `bevy_tree_sitter`), LSP UI (via `bevy_lsp`).

## Reading state

All state is plain ECS — query it from any system:

```rust
fn status_bar(editors: Query<(&TextBuffer, &CursorState, &FoldState), With<CodeEditor>>) {
    for (buffer, cursor, folds) in &editors { /* … */ }
}
```

File I/O events are public — emit `SaveRequested` / `OpenRequested` yourself or handle them in your own system.

## Embedding in a larger app

Disable sub-plugins your host already provides:

```rust
CodeEditorPlugins.build().disable::<EditorUiPlugin>()
```

Override components at spawn:

```rust
commands.spawn((CodeEditor, FontConfig::from_size(18.0), TextBuffer::with_text("fn main() {}")));
```

## Feature flags

- `tree-sitter` (default) — syntax highlighting
- `lsp` — language server integration
- `clipboard` (default) — system clipboard

## License

MIT OR Apache-2.0
