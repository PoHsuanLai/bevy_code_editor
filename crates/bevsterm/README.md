# bevsterm

Embeddable PTY-backed terminal for Bevy. Spawn `BevyTerminal` into any app and it runs as a normal ECS entity.

## Quick start

```rust
use bevy::prelude::*;
use bevsterm::prelude::*;
use bevy_instanced_text::InstancedTextPlugins;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, InstancedTextPlugins, BevyTerminalPlugin))
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(BevyTerminal);
        })
        .run();
}
```

## Features

Real PTY, VT100/VT220 + xterm extensions, 256-color + truecolor, drag-select, copy/paste, per-entity theme.

## Reading state

```rust
fn tab_bar(terminals: Query<(&TerminalShellInfo, &TerminalGridSnapshot), With<BevyTerminal>>) {
    for (info, grid) in &terminals {
        // info.title, info.cwd, grid.cols, grid.rows, grid.cursor_row
    }
}
```

| Component | What it holds |
|---|---|
| `TerminalGridSnapshot` | Grid dimensions and cursor position. |
| `TerminalShellInfo` | Title and CWD from OSC 0/1/2/7. |
| `TerminalBlockState` | OSC 133 command blocks with exit codes. |
| `TerminalScrollFollow` | Whether the view is pinned to the bottom. |
| `TerminalColorPalette` | 16 ANSI colors — mutate to retheme at runtime. |

## Messages

- **Outbound:** `TerminalReady`, `TerminalExited`, `TerminalTitleChanged`, `TerminalBellRang`, `TerminalCwdChanged`, `TerminalBlockFinished`.
- **Inbound:** `TerminalWriteBytes`, `TerminalRunCommand`, `TerminalResize`, `TerminalScrollTo`, `TerminalClear`, `TerminalCopySelection`, `TerminalPaste`.

## License

MIT OR Apache-2.0
