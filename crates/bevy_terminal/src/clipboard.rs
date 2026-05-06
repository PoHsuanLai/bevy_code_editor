//! Inbound message handlers: clipboard ops and direct host commands.
//!
//! Reads the inbound message bus (`TerminalCopySelection`, `TerminalPaste`,
//! `TerminalWriteBytes`, `TerminalRunCommand`, `TerminalResize`,
//! `TerminalScrollTo`, `TerminalClear`) and translates them into PTY
//! writes plus `Terminal` mutations. Bytes that should be visible to the
//! shell (raw writes, run-command, paste) flow through the PTY master
//! writer; bytes that should mutate terminal state (clear) walk
//! `Terminal::advance_bytes`.

use bevy::prelude::*;
use bevy_text_editor::{copy_selection, SelectionState};
use bevy_text_engine::TextBuffer;
use portable_pty::PtySize;

use crate::messages::{
    TerminalClear, TerminalCopySelection, TerminalPaste, TerminalResize, TerminalRunCommand,
    TerminalScrollTo, TerminalWriteBytes,
};
use crate::types::{TerminalGridSnapshot, TerminalSession};

/// Handle `TerminalCopySelection`: read the entity's selection from
/// `SelectionState` and call into the shared `copy_selection` helper
/// (which honors `SelectionMode` for block / line / semantic copies).
pub fn handle_copy_selection(
    mut events: MessageReader<TerminalCopySelection>,
    q: Query<(&SelectionState, &TextBuffer)>,
) {
    for ev in events.read() {
        let Ok((sel, buffer)) = q.get(ev.entity) else {
            continue;
        };
        let _ = copy_selection(sel, buffer);
    }
}

/// Handle `TerminalPaste`: hand the text to wezterm's `send_paste`,
/// which wraps in bracketed-paste markers when the mode is enabled and
/// canonicalizes newlines per the configured `NewlineCanon`.
pub fn handle_paste(
    mut events: MessageReader<TerminalPaste>,
    q: Query<&TerminalSession>,
) {
    for ev in events.read() {
        let Ok(session) = q.get(ev.entity) else {
            continue;
        };
        let _ = session.terminal.lock().send_paste(&ev.text);
    }
}

/// Handle raw-byte writes (the firehose path; no interpretation).
/// Bytes go through the shared PTY-input writer so the shell sees them
/// as if typed; the `Terminal` parser receives them on the next drain
/// tick via the reader thread (the shell's echo path).
pub fn handle_write_bytes(
    mut events: MessageReader<TerminalWriteBytes>,
    q: Query<&TerminalSession>,
) {
    for ev in events.read() {
        let Ok(session) = q.get(ev.entity) else {
            continue;
        };
        let _ = session.pty_input.write_bytes(&ev.bytes);
    }
}

/// Handle `TerminalRunCommand`: append the command + `\r` (Enter), via
/// the shared PTY-input writer.
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
        let _ = session.pty_input.write_bytes(&bytes);
    }
}

/// Handle `TerminalResize`: reshape the wezterm `Terminal` grid and
/// tell the PTY master about the new (cols, rows). Cell-pixel hints are
/// kept from the last viewport-driven resize.
pub fn handle_resize(
    mut events: MessageReader<TerminalResize>,
    mut q: Query<&mut TerminalSession>,
) {
    for ev in events.read() {
        let Ok(mut session) = q.get_mut(ev.entity) else {
            continue;
        };
        if ev.cols == 0 || ev.rows == 0 {
            continue;
        }
        let cell_w = (session.size.pixel_width / session.size.cols.max(1)) as u16;
        let cell_h = (session.size.pixel_height / session.size.rows.max(1)) as u16;
        let new_size = crate::backend::TerminalSize {
            cols: ev.cols as usize,
            rows: ev.rows as usize,
            pixel_width: (ev.cols * cell_w) as usize,
            pixel_height: (ev.rows * cell_h) as usize,
            dpi: session.size.dpi,
        };
        let pty_size = PtySize {
            cols: ev.cols,
            rows: ev.rows,
            pixel_width: ev.cols * cell_w,
            pixel_height: ev.rows * cell_h,
        };
        session.terminal.lock().resize(new_size);
        let _ = session.pty_master.lock().resize(pty_size);
        session.size = new_size;
    }
}

/// Handle `TerminalScrollTo`: nudge the visible window so `line` (a
/// stable buffer row) lands at the top. Wezterm tracks scroll position
/// via `Screen::scrollback_top` plus the visible-row range; the simplest
/// portable hook is to walk the parser through a vertical-position CSI
/// (`ESC[r`) — but that mutates state. Instead, we leave the scroll
/// state to the host's overlay logic and just bump the snapshot version
/// so the renderer redraws.
pub fn handle_scroll_to(
    mut events: MessageReader<TerminalScrollTo>,
    mut q: Query<&mut TerminalGridSnapshot>,
) {
    for ev in events.read() {
        let Ok(mut snapshot) = q.get_mut(ev.entity) else {
            continue;
        };
        let _ = ev.line;
        snapshot.version = snapshot.version.wrapping_add(1);
    }
}

/// Handle `TerminalClear`: dispatch the standard clear sequences
/// (`ESC[3J ESC[2J ESC[H`) through the parser so it walks the same code
/// path the shell's `clear` would.
pub fn handle_clear(
    mut events: MessageReader<TerminalClear>,
    mut q: Query<(&TerminalSession, &mut TerminalGridSnapshot)>,
) {
    const CLEAR_SEQUENCE: &[u8] = b"\x1b[3J\x1b[2J\x1b[H";
    for ev in events.read() {
        let Ok((session, mut snapshot)) = q.get_mut(ev.entity) else {
            continue;
        };
        session.terminal.lock().advance_bytes(CLEAR_SEQUENCE);
        snapshot.version = snapshot.version.wrapping_add(1);
    }
}
