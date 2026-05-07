//! Inbound message handlers: clipboard ops + direct host commands.

use bevy::prelude::*;
use bevy_text_editor::{copy_selection, SelectionState};
use bevy_text_engine::{FontConfig, ScrollState, TextBuffer};
use portable_pty::PtySize;

use crate::messages::{
    TerminalClear, TerminalCopySelection, TerminalPaste, TerminalResize, TerminalRunCommand,
    TerminalScrollTo, TerminalScrollToBottom, TerminalScrollToTop, TerminalWriteBytes,
};
use crate::types::{TerminalGridSnapshot, TerminalScrollFollow, TerminalSession};

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

/// Jump the viewport so `line` (0 = top of scrollback, growing downward) sits
/// at the top of the visible area. Disengages bottom-follow so the user stays
/// where they jumped to as new output arrives.
pub fn handle_scroll_to(
    mut events: MessageReader<TerminalScrollTo>,
    mut q: Query<(&mut ScrollState, &mut TerminalScrollFollow, &FontConfig)>,
) {
    for ev in events.read() {
        let Ok((mut scroll, mut follow, font)) = q.get_mut(ev.entity) else {
            continue;
        };
        let target = -(ev.line.max(0) as f32 * font.line_height);
        scroll.scroll_offset = target;
        scroll.target_scroll_offset = target;
        follow.stick_to_bottom = false;
        follow.last_applied_target = target;
    }
}

/// Re-engage bottom-follow and snap to the latest output. The next
/// `sync_grid_snapshot` tick will write the bottom-anchored offsets.
pub fn handle_scroll_to_bottom(
    mut events: MessageReader<TerminalScrollToBottom>,
    mut q: Query<&mut TerminalScrollFollow>,
) {
    for ev in events.read() {
        let Ok(mut follow) = q.get_mut(ev.entity) else {
            continue;
        };
        follow.stick_to_bottom = true;
        // Force `anchor_scroll_to_bottom`'s wheel-detector to skip — by leaving
        // `last_applied_target` alone, `target_scroll_offset` will look "in
        // sync" until the system rewrites both to the new bottom.
    }
}

/// Jump to the top of the buffer and disengage bottom-follow.
pub fn handle_scroll_to_top(
    mut events: MessageReader<TerminalScrollToTop>,
    mut q: Query<(&mut ScrollState, &mut TerminalScrollFollow)>,
) {
    for ev in events.read() {
        let Ok((mut scroll, mut follow)) = q.get_mut(ev.entity) else {
            continue;
        };
        scroll.scroll_offset = 0.0;
        scroll.target_scroll_offset = 0.0;
        follow.stick_to_bottom = false;
        follow.last_applied_target = 0.0;
    }
}

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
