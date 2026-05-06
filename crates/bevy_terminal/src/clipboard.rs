//! Clipboard + raw-write message handlers.
//!
//! Reads `TerminalCopySelection` / `TerminalPaste` / `TerminalWriteBytes`
//! / `TerminalRunCommand` and translates them into PTY writes (and
//! clipboard operations via `bevy_text_editor::copy_selection`).

use alacritty_terminal::event::Notify;
use bevy::prelude::*;
use bevy_text_editor::{copy_selection, SelectionState};
use bevy_text_engine::TextViewState;

use crate::messages::{
    TerminalCopySelection, TerminalPaste, TerminalRunCommand, TerminalWriteBytes,
};
use crate::types::{TerminalInputMode, TerminalSession};

/// Handle `TerminalCopySelection`: read the entity's selection from
/// `SelectionState` and call into the shared `copy_selection` helper
/// (which honors `SelectionMode` for block / line / semantic copies).
pub fn handle_copy_selection(
    mut events: MessageReader<TerminalCopySelection>,
    q: Query<(&SelectionState, &TextViewState)>,
) {
    for ev in events.read() {
        let Ok((sel, tv)) = q.get(ev.entity) else {
            continue;
        };
        let _ = copy_selection(sel, tv);
    }
}

/// Handle `TerminalPaste`: write the bytes to the PTY, wrapped in
/// bracketed-paste markers when the term has the mode enabled.
pub fn handle_paste(
    mut events: MessageReader<TerminalPaste>,
    q: Query<(&TerminalSession, &TerminalInputMode)>,
) {
    for ev in events.read() {
        let Ok((session, mode)) = q.get(ev.entity) else {
            continue;
        };
        let bytes = if mode.bracketed_paste {
            let mut out =
                Vec::with_capacity(ev.text.len() + BRACKETED_PASTE_START.len() + BRACKETED_PASTE_END.len());
            out.extend_from_slice(BRACKETED_PASTE_START);
            out.extend_from_slice(ev.text.as_bytes());
            out.extend_from_slice(BRACKETED_PASTE_END);
            out
        } else {
            ev.text.as_bytes().to_vec()
        };
        session.notifier.notify(bytes);
    }
}

/// Handle raw-byte writes (the firehose path; no interpretation).
pub fn handle_write_bytes(
    mut events: MessageReader<TerminalWriteBytes>,
    q: Query<&TerminalSession>,
) {
    for ev in events.read() {
        let Ok(session) = q.get(ev.entity) else {
            continue;
        };
        session.notifier.notify(ev.bytes.clone());
    }
}

/// Handle `TerminalRunCommand`: append the command + `\r` (Enter).
pub fn handle_run_command(
    mut events: MessageReader<TerminalRunCommand>,
    q: Query<&TerminalSession>,
) {
    for ev in events.read() {
        let Ok(session) = q.get(ev.entity) else {
            continue;
        };
        let mut bytes = Vec::with_capacity(ev.command.len() + 1);
        bytes.extend_from_slice(ev.command.as_bytes());
        bytes.push(b'\r');
        session.notifier.notify(bytes);
    }
}

const BRACKETED_PASTE_START: &[u8] = b"\x1b[200~";
const BRACKETED_PASTE_END: &[u8] = b"\x1b[201~";
