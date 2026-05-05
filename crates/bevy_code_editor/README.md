# bevy_code_editor

A code editor as a Bevy plugin, layered on top of [`bevy_text_engine`](../bevy_text_engine) (rendering) and [`bevy_text_editor`](../bevy_text_editor) (editable-text widget).

This crate adds the IDE-specific extras: multi-cursor, syntax-highlight adapter (over [`bevy_tree_sitter`](../bevy_tree_sitter)), LSP UI adapter (over [`bevy_lsp`](../bevy_lsp)), folding, bracket matching, scrollbar, line numbers, gutter, goto-line dialog. The cursor / selection / edit history / undo / clipboard machinery lives one tier down in `bevy_text_editor` and is shared with simpler hosts (chat boxes, search fields).

## Hello-world

```rust
use bevy::prelude::*;
use bevy_code_editor::prelude::*;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, CodeEditorPlugin::standalone()))
        .run();
}
```

`CodeEditorPlugin::standalone()` is a `PluginGroup` that bundles the engine, interaction, the editor, and the default UI plugin (line numbers, separator, camera). One line, working editor.

## Embedded in a larger app

Drop `CodeEditorPlugin` into an app that already has its own camera and input. Plugins are explicit and additive — no auto-add of unrelated machinery:

```rust
App::new()
    .add_plugins((
        DefaultPlugins,
        MyGamePlugin,
        TextEnginePlugins,         // engine PluginGroup
        TextInteractionPlugin,     // mouse/scroll/select/copy
        CodeEditorPlugin,          // the editor
    ))
    .add_systems(Startup, |mut commands: Commands| {
        commands.spawn(CodeEditor::default());
    })
    .run();
```

Spawning is one component, like `Text2d` — `#[require(...)]` cascades the supporting components (`CursorState`, `SelectionState`, `EditHistoryState`, `FoldState`, `BracketMatchState`, plus engine-side `TextView` / `FontConfig` / `DisplayLayout`). Override anything by passing extra components in the bundle:

```rust
commands.spawn((
    CodeEditor,
    FontConfig::from_size(18.0).with_line_height_multiplier(1.4),
    TextViewState::with_text("fn main() {}"),
));
```

## Architecture

```
EditorAction (leafwing-input-manager)
    ↓
dispatch_action_events (one big match → 46 typed *Requested events)
    ↓
per-action handler systems (cursor_move, selection, edit, clipboard,
                            multi_cursor, folding, file, lsp)
    ↓
buffer (rope) + cursor / selection state
    ↓
display_map: produce_layouts (engine system) reads HiddenLines +
             LineStyles plain-data Components → DisplayLayout
    ↓
TextEnginePlugin: render_layout → GlyphInstance → instanced GPU draw
```

Three places worth knowing about for hosts:

- **`input/dispatch.rs`** — leafwing `EditorAction` poll fans out into typed `*Requested` events. Per-action handler systems consume those events. Hosts that want to override behavior can send the events themselves, or run a system between the dispatcher and a specific handler that intercepts them.
- **`display_map/plugin.rs`** — producer systems `produce_hidden_lines` (writes `HiddenLines` from `FoldState`) and `produce_line_styles` (writes `LineStyles` from `EditorSyntaxState` for the visible buffer-line window). Both run in `LayoutSyncSet`, before the engine's `LayoutProduceSet`. Helpers in `display_map/styling.rs` convert the editor's `LineSegment` shape into the engine's `RunWithText`.
- **`lsp_ui/`** (feature `lsp`) — observes `bevy_lsp::LspResponse` messages, drives completion / hover popup state, renders. Popup nav (Up / Down / Enter / Tab / Escape) intercepts the corresponding `*Requested` events when the popup is visible.

## Two editors at once

```rust
fn spawn_two(mut commands: Commands) {
    commands.spawn((
        CodeEditor,
        TextViewViewport { rect: Rect::new(0.0, 0.0, 800.0, 600.0), .. },
    ));
    commands.spawn((
        CodeEditor,
        TextViewViewport { rect: Rect::new(800.0, 0.0, 1600.0, 600.0), .. },
    ));
}
```

`bevy_picking` routes mouse to whichever editor is hovered. `bevy_input_focus` routes keyboard to whichever was clicked last. Each editor has its own font, scroll, fold state, syntax tree, LSP client. See `examples/multi_editor.rs`.

## Feature flags

- `tree-sitter` (default) — pulls in `bevy_tree_sitter` + the syntax-highlight adapter (`syntax/`).
- `lsp` — pulls in `bevy_lsp` + the LSP UI adapter (`lsp_ui/`).
- `clipboard` (default) — system clipboard via `arboard` (always-on; pulled into `bevy_text_editor`).

Minimal build (no syntax highlighting, no LSP):

```bash
cargo build -p bevy_code_editor --no-default-features
```

## Customizing key bindings

Spawn an `EditorInputManager` with your own `InputMap<EditorAction>` *before* `PostStartup`; the plugin's default input manager is gated on no existing one being present.

```rust
fn setup_keys(mut commands: Commands) {
    let mut input_map = InputMap::<EditorAction>::default();
    input_map.insert(EditorAction::SaveFile, KeyCode::F2);
    commands.spawn(EditorInputManager::with(input_map));
}

App::new()
    .add_plugins(CodeEditorPlugin::standalone())
    .add_systems(PreStartup, setup_keys)
    .run();
```

## File save / open

`SaveRequested { entity, path }` and `OpenRequested { entity, path }` are public events emitted by the `EditorAction::SaveFile` / `OpenFile` actions. The editor doesn't read or write files itself — host the file dialog in your app.

```rust
fn handle_save(
    mut events: MessageReader<SaveRequested>,
    q: Query<&TextViewState>,
) {
    for SaveRequested { entity, path } in events.read() {
        if let Ok(state) = q.get(*entity) {
            std::fs::write(path, state.rope.to_string()).unwrap();
        }
    }
}
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
