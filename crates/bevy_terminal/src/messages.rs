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
