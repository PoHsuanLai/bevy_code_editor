# bevsterm

Embeddable PTY-backed terminal for Bevy. Spawn `BevyTerminal` into any app and it runs as a normal ECS entity — a game HUD, a dev tool, a split-pane layout alongside an editor.

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

## What you get

- **Real PTY** — the user's `$SHELL` runs in the background; output streams to the cell grid; resize delivers `SIGWINCH`.
- **VT100/VT220 + xterm extensions** — alt-screen apps (`vim`, `htop`, `less`), arrow keys, function keys, mouse reporting, kitty keyboard.
- **256-color + truecolor** rendering via `bevy_instanced_text`.
- **Drag-select** (single / double / triple click + Alt drag).
- **Cmd+C / Cmd+V** (or **Ctrl+Shift+C / Ctrl+Shift+V** on Linux/Windows).
- **Per-entity theme** via `TerminalThemeConfig` — 16-slot ANSI palette + block-background colors.

## Reading terminal state

The plugin writes these components every frame — query them from any system, no special API:

```rust
fn my_tab_bar(terminals: Query<(&TerminalShellInfo, &TerminalGridSnapshot), With<BevyTerminal>>) {
    for (info, grid) in &terminals {
        // info.title, info.cwd — from OSC 0/1/2/7
        // grid.cols, grid.rows, grid.cursor_row, grid.cursor_col
    }
}
```

| Component | What it holds |
|---|---|
| `TerminalGridSnapshot` | Grid dimensions and cursor position. |
| `TerminalShellInfo` | Title and CWD from OSC 0/1/2/7. |
| `TerminalBlockState` | OSC 133 command blocks: command, output row range, exit code. |
| `TerminalScrollFollow` | Whether the view is pinned to the bottom. |
| `TerminalColorPalette` | The 16 ANSI colors — mutate to retheme at runtime. |

## ECS shape

`BevyTerminal` is a marker; `#[require]` cascades the rest automatically:

| Component | Purpose |
|---|---|
| `TerminalSession` | PTY session handle (write side). |
| `TerminalEventChannel` | crossbeam receiver from the PTY event-loop thread. |
| `TerminalThemeConfig` | 16-color ANSI palette + block bgs. |
| `TerminalScrollback` | Max scrollback lines. |
| `TerminalCursorBlink` | Caret blink phase state. |

## Messages

- **Outbound:** `TerminalReady`, `TerminalExited`, `TerminalTitleChanged`, `TerminalBellRang`, `TerminalCwdChanged`, `TerminalBlockFinished`.
- **Inbound:** `TerminalWriteBytes`, `TerminalRunCommand`, `TerminalResize`, `TerminalScrollTo`, `TerminalClear`, `TerminalCopySelection`, `TerminalPaste`.

## System sets

```
TerminalPtyDrainSet     ← drain PTY receiver, mirror term mode
TerminalApplyStateSet   ← clipboard handlers, viewport-driven resize
TerminalSnapshotSet     ← grid → LineStyles + GridSnapshot + caret
```

## Examples

- `examples/basic_terminal.rs` — single full-window terminal.
- `examples/editor_and_terminal.rs` — split-pane: code editor + terminal.

## Roadmap

- OSC 133 command blocks (Warp-style framing).
- Shell integration scripts (`bash`/`zsh`/`fish`/`pwsh`).
- Sixel / iTerm2 inline images.
- Multi-line search inside scrollback.

## License

MIT OR Apache-2.0
