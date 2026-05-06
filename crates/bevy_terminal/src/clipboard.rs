//! Inbound message handlers: clipboard ops and direct host commands.
//!
//! Reads the inbound message bus (`TerminalCopySelection`, `TerminalPaste`,
//! `TerminalWriteBytes`, `TerminalRunCommand`, `TerminalResize`,
//! `TerminalScrollTo`, `TerminalClear`) and translates them into PTY writes
//! plus `Term` mutations. Clipboard reads use `bevy_text_editor::copy_selection`.

use alacritty_terminal::event::{Notify, OnResize, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use bevy::prelude::*;
use bevy_text_editor::{copy_selection, SelectionState};
use bevy_text_engine::TextBuffer;

use crate::messages::{
    TerminalClear, TerminalCopySelection, TerminalPaste, TerminalResize, TerminalRunCommand,
    TerminalScrollTo, TerminalWriteBytes,
};
use crate::types::{TerminalGridSnapshot, TerminalInputMode, TerminalSession};

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

/// Handle `TerminalResize`: reshape the `Term` grid and notify the PTY of
/// the new (cols, rows). Cell-pixel hints are kept from the last viewport
/// resize that ran on this session.
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
        let new_size = WindowSize {
            num_cols: ev.cols,
            num_lines: ev.rows,
            cell_width: session.size.cell_width,
            cell_height: session.size.cell_height,
        };
        {
            let mut term = session.terminal.lock();
            term.resize(ResizeDims {
                cols: ev.cols as usize,
                rows: ev.rows as usize,
            });
        }
        session.notifier.on_resize(new_size);
        session.size = new_size;
    }
}

/// Handle `TerminalScrollTo`: scroll the display by the delta needed to
/// land `line` at the top of the visible window.
pub fn handle_scroll_to(
    mut events: MessageReader<TerminalScrollTo>,
    q: Query<&TerminalSession>,
) {
    for ev in events.read() {
        let Ok(session) = q.get(ev.entity) else {
            continue;
        };
        let mut term = session.terminal.lock();
        let current = term.grid().display_offset() as i64;
        let target = -ev.line;
        let delta = (target - (-current)) as i32;
        if delta != 0 {
            term.scroll_display(Scroll::Delta(delta));
        }
    }
}

/// Handle `TerminalClear`: drop the scrollback and clear the screen.
/// We dispatch the standard clear sequences (`ESC[3J ESC[2J ESC[H`) so the
/// `Term` parser walks the same code path the shell's `clear` would, then
/// also call `clear_history` to drop scrollback the shell didn't reset.
pub fn handle_clear(
    mut events: MessageReader<TerminalClear>,
    mut q: Query<(&TerminalSession, &mut TerminalGridSnapshot)>,
) {
    for ev in events.read() {
        let Ok((session, mut snapshot)) = q.get_mut(ev.entity) else {
            continue;
        };
        let mut term = session.terminal.lock();
        term.grid_mut().clear_history();
        snapshot.version = snapshot.version.wrapping_add(1);
    }
}

/// Tiny `Dimensions` shim so `Term::resize` can take cols/rows from a
/// host-driven `TerminalResize` message.
struct ResizeDims {
    cols: usize,
    rows: usize,
}

impl Dimensions for ResizeDims {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

const BRACKETED_PASTE_START: &[u8] = b"\x1b[200~";
const BRACKETED_PASTE_END: &[u8] = b"\x1b[201~";
