//! Bevy `Message` types: the inter-system bus for the terminal crate.
//!
//! Three flavors:
//! - **Outbound** — emitted by the drain system, app reacts.
//! - **Inbound** — emitted by app code, terminal systems react.
//! - **Internal** — `TerminalAction` (enum dispatched by leafwing), fanned
//!   out into the typed inbound messages by `crate::input::dispatch`.
//!
//! All `#[derive(Message, Reflect)]` and registered via `add_message` +
//! `register_type` in `TerminalPlugin::build`.

use bevy::prelude::*;
use std::process::ExitStatus;

use crate::backend::{KeyCode as TerminalKeyCode, KeyModifiers as TerminalKeyModifiers};

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
pub struct TerminalBell {
    pub entity: Entity,
}

/// PTY is up and the initial dimensions are known. Emitted once per session
/// once the writer + reader threads are wired.
#[derive(Message, Clone, Debug, Reflect)]
pub struct TerminalReady {
    pub entity: Entity,
    pub cols: u16,
    pub rows: u16,
}

/// PTY allocation or shell spawn failed. Emitted once for the offending
/// entity; the entity is despawned in the same system. Hosts can subscribe
/// to surface a UI error or retry with a different `TerminalConfig`.
#[derive(Message, Clone, Debug, Reflect)]
pub struct TerminalSpawnFailed {
    pub entity: Entity,
    /// Human-readable error from the PTY backend / OS.
    pub error: String,
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

/// Bottom-follow state flipped (user wheeled away from the bottom or wheeled
/// back into it). Hosts can wire this to a "Jump to bottom" UI affordance.
/// Equivalent to a `Changed<TerminalScrollFollow>` query — provided as a
/// message for hosts that prefer the bus.
#[derive(Message, Clone, Debug, Reflect)]
pub struct TerminalScrollFollowChanged {
    pub entity: Entity,
    pub stick_to_bottom: bool,
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
/// at the top of the visible area. Implicitly disengages bottom-follow.
#[derive(Message, Clone, Debug, Reflect)]
pub struct TerminalScrollTo {
    pub entity: Entity,
    pub line: i64,
}

/// Re-engage bottom-follow: snap to the latest output and keep the viewport
/// pinned there as new bytes arrive. Hosts wire this to a "jump to bottom"
/// affordance (button, shortcut, gesture).
#[derive(Message, Clone, Debug, Reflect)]
pub struct TerminalScrollToBottom {
    pub entity: Entity,
}

/// Jump to the top of the buffer (oldest scrollback). Disengages
/// bottom-follow.
#[derive(Message, Clone, Debug, Reflect)]
pub struct TerminalScrollToTop {
    pub entity: Entity,
}

/// Clear the screen + scrollback (equivalent to running `clear` in the shell).
#[derive(Message, Clone, Debug, Reflect)]
pub struct TerminalClear {
    pub entity: Entity,
}

/// Synthesize a keypress on the terminal. Equivalent to the user typing it:
/// the terminal encodes the key according to its current input mode (cursor
/// keys, kitty keyboard, etc.) and writes the bytes to the PTY. Hosts use
/// this for macros, command palettes that send `Esc`, or "send Ctrl-C"
/// affordances.
#[derive(Message, Clone, Debug)]
pub struct TerminalKeyInput {
    pub entity: Entity,
    pub key: TerminalKeyCode,
    pub mods: TerminalKeyModifiers,
}

/// POSIX signal to forward to the PTY's child process group. Use
/// `libc::SIGINT` etc. on Unix; ignored on Windows. Useful for "stop this
/// command" buttons that don't want to wait for the user to focus the pane
/// and press Ctrl-C.
#[derive(Message, Clone, Debug, Reflect)]
pub struct TerminalSendSignal {
    pub entity: Entity,
    pub signal: i32,
}

/// Move keyboard focus to this terminal entity. Equivalent to setting
/// Bevy's `InputFocus` directly, exposed on the bus so host UIs can
/// programmatically focus a pane (split layouts, "next terminal" shortcuts).
#[derive(Message, Clone, Debug, Reflect)]
pub struct TerminalFocus {
    pub entity: Entity,
}
