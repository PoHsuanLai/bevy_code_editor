# bevy_terminal

Embeddable terminal widget for Bevy. Spawn `BevyTerminal`, get a working
shell.

```rust
use bevy::prelude::*;
use bevy_terminal::prelude::*;
use bevy_text_engine::TextEnginePlugins;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(TextEnginePlugins)
        .add_plugins(BevyTerminalPlugin)
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(BevyTerminal);
        })
        .run();
}
```

## What you get

- **Real PTY** via `alacritty_terminal` — the user's `$SHELL` runs in the
  background; output streams to the cell grid; resize delivers `SIGWINCH`.
- **VT100/VT220 + xterm extensions** — alt-screen apps (`vim`, `htop`,
  `less`), arrow keys, function keys, mouse reporting, kitty keyboard.
- **256-color + truecolor** rendering through `bevy_text_engine`.
- **Drag-select with multi-mode** (single / double / triple click + Alt
  drag) thanks to the lifted `SelectionMode` in `bevy_text_editor`.
- **Cmd+C / Cmd+V** (or **Ctrl+Shift+C / Ctrl+Shift+V** on Linux/Windows)
  copy and bracketed-paste.
- **Per-entity theme** via `TerminalThemeConfig` — 16-slot ANSI palette
  + block-background colors; render-side colors come from the engine's
  `RenderTheme`.

## ECS shape

`BevyTerminal` is a marker; `#[require]` cascades the rest:

| Component | Purpose |
|---|---|
| `TerminalSession` | `Arc<FairMutex<Term>>` + `Notifier` (PTY write side). |
| `TerminalEventChannel` | crossbeam receiver from the alacritty event-loop thread. |
| `TerminalGridSnapshot` | Cell-grid metadata (cols, rows, cursor). |
| `TerminalShellInfo` | Title + cwd from OSC 0/1/2/7. |
| `TerminalInputMode` | Mirror of `TermMode` flags (cursor-key app, alt-screen, mouse). |
| `TerminalBlockState` | Reserved for OSC 133 command blocks (deferred). |
| `TerminalThemeConfig` | 16-color ANSI palette + block bgs. |
| `TerminalScrollback` | Max scrollback lines. |
| `TerminalCursorBlink` | Phase-reset state for the caret. |
| (engine substrate) | `TextView`, `TextViewState`, `LineStyles`, … |
| (text_editor) | `SelectionState`, `EditTheme`, `ScrollConfig`. |

## Messages

- **Outbound:** `TerminalReady`, `TerminalExited`, `TerminalTitleChanged`,
  `TerminalBellRang`, `TerminalCwdChanged`, `TerminalBlockFinished`.
- **Inbound:** `TerminalWriteBytes`, `TerminalRunCommand`,
  `TerminalResize`, `TerminalScrollTo`, `TerminalClear`,
  `TerminalCopySelection`, `TerminalPaste`.

All registered with `add_message::<T>()` and `register_type::<T>()`.

## System sets

```
TerminalPtyDrainSet     ← drain crossbeam receiver, mirror term mode
TerminalApplyStateSet   ← clipboard handlers, viewport-driven resize
TerminalSnapshotSet     ← Term grid → LineStyles + GridSnapshot + caret
                          (engine's LayoutProduceSet runs after this)
```

Keyboard input is an observer on `FocusedInput<KeyboardInput>`, not a
system, so it doesn't sit in a set.

## Examples

- `examples/basic_terminal.rs` — single full-window terminal.
- `examples/editor_and_terminal.rs` — split-pane: code editor + terminal.

## Roadmap

- OSC 133 command blocks (Warp-style framing) — needs a parallel
  `vte::Parser` tee'd off the PTY byte stream; deferred for v1.
- Shell integration scripts (`bash`/`zsh`/`fish`/`pwsh`) for OSC 133.
- Sixel / iTerm2 inline images.
- Multi-line search inside scrollback.
