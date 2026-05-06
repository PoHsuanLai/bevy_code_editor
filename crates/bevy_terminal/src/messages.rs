//! Bevy `Message` types: the inter-system bus for the terminal crate.
//!
//! Three flavors:
//! - **Outbound** — emitted by the drain system, app reacts.
//! - **Inbound** — emitted by app code, terminal systems react.
//! - **Internal** — `TerminalAction` (enum dispatched by leafwing), fanned
//!   out into the typed inbound messages by `crate::input::dispatch`.
//!
//! All `#[derive(Message, Reflect)]` and registered via `add_message` +
//! `register_type` in `BevyTerminalPlugin::build`.

use bevy::prelude::*;
use std::process::ExitStatus;

// ─── Outbound (terminal → app) ──────────────────────────────────────────

/// Child process exited (for whatever reason).
#[derive(Message, Clone, Debug, Reflect)]
#[reflect(Debug)]
pub struct TerminalExited {
    pub entity: Entity,
    /// `None` when the OS didn't deliver a status (e.g. detached PTY).
    #[reflect(ignore)]
    pub status: Option<ExitStatusReflect>,
}

/// Reflect-friendly `ExitStatus` wrapper. We can't `Reflect` `ExitStatus`
/// directly; the exit code is the only piece anyone cares about.
#[derive(Clone, Debug)]
pub struct ExitStatusReflect(pub ExitStatus);

/// OSC 0/1/2 — title change.
#[derive(Message, Clone, Debug, Reflect)]
pub struct TerminalTitleChanged {
    pub entity: Entity,
    pub title: String,
}

/// BEL (`\a`) ringing.
#[derive(Message, Clone, Debug, Reflect)]
pub struct TerminalBellRang {
    pub entity: Entity,
}

/// PTY is up and the initial dimensions are known. Emitted once per session
/// from `on_terminal_added` once the writer + reader threads are wired.
#[derive(Message, Clone, Debug, Reflect)]
pub struct TerminalReady {
    pub entity: Entity,
    pub cols: u16,
    pub rows: u16,
}

/// OSC 7 — shell announced a new working directory.
#[derive(Message, Clone, Debug, Reflect)]
pub struct TerminalCwdChanged {
    pub entity: Entity,
    pub cwd: String,
}

/// OSC 133 D — a command block transitioned to the completed state.
/// Carries the block id and the optional exit code parsed from the sequence.
#[derive(Message, Clone, Debug, Reflect)]
pub struct TerminalBlockFinished {
    pub entity: Entity,
    pub block_id: u64,
    pub exit_code: Option<i32>,
}

/// User clicked one of the per-block `Pickable` child entities.
#[derive(Message, Clone, Debug, Reflect)]
pub struct TerminalBlockSelected {
    pub entity: Entity,
    pub block_id: u64,
}

// ─── Inbound (app → terminal) ───────────────────────────────────────────

/// Write raw bytes to the PTY (the firehose path — no interpretation).
#[derive(Message, Clone, Debug, Reflect)]
pub struct TerminalWriteBytes {
    pub entity: Entity,
    pub bytes: Vec<u8>,
}

/// Convenience: append a string + Enter, like the user typed it.
#[derive(Message, Clone, Debug, Reflect)]
pub struct TerminalRunCommand {
    pub entity: Entity,
    pub command: String,
}

/// Copy current selection to the clipboard.
#[derive(Message, Clone, Debug, Reflect)]
pub struct TerminalCopySelection {
    pub entity: Entity,
}

/// Paste arbitrary text into the PTY (bracketed if mode is set).
#[derive(Message, Clone, Debug, Reflect)]
pub struct TerminalPaste {
    pub entity: Entity,
    pub text: String,
}

/// Force a (cols, rows) on the terminal without owning the viewport.
/// Useful for hosts that drive layout themselves (split panes, tiled WMs).
#[derive(Message, Clone, Debug, Reflect)]
pub struct TerminalResize {
    pub entity: Entity,
    pub cols: u16,
    pub rows: u16,
}

/// Scroll the terminal so `line` (a buffer row, 0 = top of scrollback) is
/// at the top of the visible area.
#[derive(Message, Clone, Debug, Reflect)]
pub struct TerminalScrollTo {
    pub entity: Entity,
    pub line: i64,
}

/// Clear the screen + scrollback (equivalent to running `clear` in the shell).
#[derive(Message, Clone, Debug, Reflect)]
pub struct TerminalClear {
    pub entity: Entity,
}
