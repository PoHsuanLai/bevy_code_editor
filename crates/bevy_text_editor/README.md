# bevy_text_editor

Editable text widget for Bevy: pointer interaction (scroll, drag-select, copy) on top of [`bevy_text_engine`](../bevy_text_engine) `TextView` entities, plus the editable-text core (cursor, selection, edit history, undo/redo, clipboard, typed-character handling).

This is the middle layer in a three-tier stack:

- [`bevy_text_engine`](../bevy_text_engine) — GPU rendering primitives.
- **`bevy_text_editor`** — interaction + editable text widget (this crate).
- [`bevy_code_editor`](../bevy_code_editor) — IDE features (multi-cursor, folding, brackets, line numbers, scrollbar UI, LSP UI, syntax highlighting).

A search box, chat composer, or URL bar uses `TextEditorPlugin` and gets a working editable text field without dragging in the IDE features.

## Plugins

The crate ships two composable plugins:

- **`TextInteractionPlugin`** — read-only interaction: pointer scroll, click + drag selection, Cmd/Ctrl+C copy. Pair with `TextEnginePlugins` for a selectable, scrollable view.
- **`TextEditorPlugin`** — adds typed-character insertion, edit history, undo/redo, cut, paste. Spawning a `TextEditor` entity gives you a working editable text field. Pulls in `TextInteractionPlugin` automatically.

The plugin idempotently adds `bevy_picking::DefaultPickingPlugins` and `bevy_input_focus::InputDispatchPlugin` if the host hasn't already.

## Architecture

Observer-driven, not polling-system-driven:

- A custom `bevy_picking` backend hit-tests the `TextViewViewport` rect of every `TextView` and produces `PointerHits`. Picking order is `1.0` (above default backends), so a text view inside a `bevy_ui` panel gets the click before the panel itself.
- Observers consume `Pointer<Press|Drag|Release|Scroll>` events that picking has already routed to the right entity. No manual cursor-position math.
- Keyboard input (Cmd/Ctrl+C copy, typed characters, edit shortcuts) is dispatched through `bevy_input_focus::FocusedInput<KeyboardInput>` to the focused editor.

## Quick start (read-only)

```rust
use bevy::prelude::*;
use bevy_text_engine::prelude::*;
use bevy_text_editor::TextInteractionPlugin;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, TextEnginePlugins, TextInteractionPlugin))
        .run();
}
```

## Quick start (editable)

```rust
use bevy::prelude::*;
use bevy_text_engine::prelude::*;
use bevy_text_editor::{TextEditor, TextEditorPlugin};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, TextEnginePlugins, TextEditorPlugin))
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(TextEditor);
        })
        .run();
}
```

`TextEditor`'s `#[require]` cascade attaches everything needed for editing: cursor state, selection state, edit history, drag state, scroll config.

## Composition with bevy_picking

If your app already uses `bevy_picking` for other entities, the text view backend coexists — it only emits hits for entities with `TextView`. To opt a particular text view out (e.g. a non-interactive watermark), add `Pickable::IGNORE`.

## Composition with bevy_input_focus

Click on a text view sets `InputFocus` to that entity. Focused keyboard events route to that entity's observers via `dispatch_focused_input::<KeyboardInput>`. Multi-editor setups: whichever editor was clicked last is focused; typing goes there.

## License

MIT OR Apache-2.0
