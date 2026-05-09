# bevy_instanced_text_edit

Editable text widget for Bevy. Adds pointer interaction, cursor, selection, edit history, undo/redo, and clipboard to `TextView` entities.

## Quick start

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

`TextEditor`'s `#[require]` cascade attaches cursor state, selection state, edit history, drag state, and scroll config automatically.

## Plugins

- **`InstancedTextInteractionPlugin`** — read-only: scroll, drag-select, copy. For selectable log viewers, terminals, etc.
- **`InstancedTextEditPlugin`** — full editing: typed input, undo/redo, cut, paste. Pulls in `InstancedTextInteractionPlugin` automatically.

## Reading state

```rust
fn cursor_pos(editors: Query<(&CursorState, &SelectionState), With<TextEditor>>) {
    for (cursor, selection) in &editors { /* row, col, selection range */ }
}
```

## License

MIT OR Apache-2.0
