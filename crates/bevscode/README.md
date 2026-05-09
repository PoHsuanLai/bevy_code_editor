# bevscode

Embeddable code editor for Bevy. Spawn `CodeEditor` into any app — a game, a dev tool, a split-pane UI — and it runs as a normal ECS entity alongside your existing world.

IDE features: multi-cursor, syntax highlighting (via [`bevy_tree_sitter`](../bevy_tree_sitter)), LSP UI (via [`bevy_lsp`](../bevy_lsp)), folding, bracket matching, scrollbar, line numbers, and gutter.

## Quick start

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

## Embedding in a larger app

Drop `CodeEditorPlugins` into any existing app. Disable sub-plugins your host already provides:

```rust
App::new()
    .add_plugins((DefaultPlugins, MyGamePlugin, CodeEditorPlugins.build().disable::<EditorUiPlugin>()))
    .add_systems(Startup, |mut commands: Commands| {
        commands.spawn(CodeEditor);
    })
    .run();
```

Override components at spawn time:

```rust
commands.spawn((
    CodeEditor,
    FontConfig::from_size(18.0).with_line_height_multiplier(1.4),
    TextBuffer::with_text("fn main() {}"),
));
```

## Reading editor state

All state is plain ECS — query it from any system:

```rust
fn status_bar(
    editors: Query<(&TextBuffer, &CursorState, &FoldState), With<CodeEditor>>,
) {
    for (buffer, cursor, folds) in &editors {
        // line count, cursor row/col, folded ranges — all plain components
    }
}
```

File save/open events are public — the editor emits them, your app handles them:

```rust
fn handle_save(mut events: EventReader<SaveRequested>, q: Query<&TextBuffer>) {
    for SaveRequested { entity, path } in events.read() {
        if let Ok(buf) = q.get(*entity) {
            std::fs::write(path, buf.rope.to_string()).unwrap();
        }
    }
}
```

## Multiple editors

```rust
fn spawn_two(mut commands: Commands) {
    commands.spawn((CodeEditor, TextViewViewport { rect: Rect::new(0.0, 0.0, 800.0, 600.0), ..default() }));
    commands.spawn((CodeEditor, TextViewViewport { rect: Rect::new(800.0, 0.0, 1600.0, 600.0), ..default() }));
}
```

`bevy_picking` routes mouse to whichever editor is hovered. `bevy_input_focus` routes keyboard to whichever was clicked last. Each editor has its own font, scroll, fold state, syntax tree, and LSP client.

## Architecture

```
EditorAction (leafwing-input-manager)
    ↓
dispatch_action_events → typed *Requested events (one per action)
    ↓
handler systems (cursor_move, selection, edit, clipboard, multi_cursor, folding, file, lsp)
    ↓
TextBuffer (rope) + CursorState / SelectionState / FoldState / …
    ↓
display_map: HiddenLines + LineStyles → DisplayLayout
    ↓
InstancedTextPlugin: GlyphInstance → instanced GPU draw
```

## Customizing key bindings

```rust
fn setup_keys(mut commands: Commands) {
    let mut map = InputMap::<EditorAction>::default();
    map.insert(EditorAction::SaveFile, KeyCode::F2);
    commands.spawn(EditorInputManager::with(map));
}
App::new()
    .add_plugins(CodeEditorPlugins)
    .add_systems(PreStartup, setup_keys)
    .run();
```

## Feature flags

- `tree-sitter` (default) — syntax highlighting via `bevy_tree_sitter`
- `lsp` — language server integration via `bevy_lsp`
- `clipboard` (default) — system clipboard via `arboard`

```bash
cargo build -p bevscode --no-default-features
```

## Examples

```bash
cargo run --example basic
cargo run --example multi_editor
cargo run --example tree-sitter
cargo run --example lsp --features lsp
cargo run --example resizable
```

## License

MIT OR Apache-2.0
