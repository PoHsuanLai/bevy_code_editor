# bevy_instanced_text_edit

[![crates.io](https://img.shields.io/crates/v/bevy_instanced_text_edit.svg)](https://crates.io/crates/bevy_instanced_text_edit)
[![docs.rs](https://docs.rs/bevy_instanced_text_edit/badge.svg)](https://docs.rs/bevy_instanced_text_edit)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/PoHsuanLai/bevscode/blob/main/LICENSE-MIT)
[![Bevy](https://img.shields.io/badge/Bevy-0.18-blue)](https://bevyengine.org)

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

## Bevy compatibility

| `bevy_instanced_text_edit` | Bevy |
|---|---|
| 0.1 | 0.18 |

## License

MIT OR Apache-2.0
