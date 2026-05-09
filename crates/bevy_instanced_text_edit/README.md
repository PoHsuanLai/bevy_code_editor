# bevy_instanced_text_edit

Editable text widget for Bevy: pointer interaction (scroll, drag-select, copy) on `TextView` entities, plus cursor, selection, edit history, undo/redo, clipboard, and typed-character handling.

## Plugins

The crate ships two composable plugins:

- **`InstancedTextInteractionPlugin`** — read-only interaction: pointer scroll, click + drag selection, Cmd/Ctrl+C copy. Pair with `InstancedTextPlugins` for a selectable, scrollable view.
- **`InstancedTextEditPlugin`** — adds typed-character insertion, edit history, undo/redo, cut, paste. Spawning a `TextEditor` entity gives you a working editable text field. Pulls in `InstancedTextInteractionPlugin` automatically.

The plugin idempotently adds `bevy_picking::DefaultPickingPlugins` and `bevy_input_focus::InputDispatchPlugin` if the host hasn't already.

## Architecture

Observer-driven, not polling-system-driven:

- A custom `bevy_picking` backend hit-tests the `TextViewViewport` rect of every `TextView` and produces `PointerHits`. Picking order is `1.0` (above default backends), so a text view inside a `bevy_ui` panel gets the click before the panel itself.
- Observers consume `Pointer<Press|Drag|Release|Scroll>` events that picking has already routed to the right entity. No manual cursor-position math.
- Keyboard input (Cmd/Ctrl+C copy, typed characters, edit shortcuts) is dispatched through `bevy_input_focus::FocusedInput<KeyboardInput>` to the focused editor.

## Quick start (read-only)

```rust
use bevy::prelude::*;
use bevy_instanced_text::prelude::*;
use bevy_instanced_text_edit::InstancedTextInteractionPlugin;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, InstancedTextPlugins, InstancedTextInteractionPlugin))
        .run();
}
```

## Quick start (editable)

```rust
use bevy::prelude::*;
use bevy_instanced_text::prelude::*;
use bevy_instanced_text_edit::{TextEditor, InstancedTextEditPlugin};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, InstancedTextPlugins, InstancedTextEditPlugin))
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
