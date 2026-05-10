//! Inbound message handlers: clipboard ops + direct host commands.

use std::collections::HashMap;

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy_instanced_text::{TextFont, ScrollState, TextBuffer};
use bevy_instanced_text_edit::{copy_selection, ClipboardResource, SelectionState};
use portable_pty::PtySize;

use crate::messages::{
    TerminalClear, TerminalCopySelection, TerminalFocus, TerminalKeyInput, TerminalPaste,
    TerminalResize, TerminalRunCommand, TerminalScrollFollowChanged, TerminalScrollTo,
    TerminalScrollToBottom, TerminalScrollToTop, TerminalSendSignal, TerminalWriteBytes,
};
use crate::types::{TerminalGridSnapshot, TerminalScrollFollow, TerminalSession};

pub fn handle_copy_selection(
    mut events: MessageReader<TerminalCopySelection>,
    clipboard: Res<ClipboardResource>,
    q: Query<(&SelectionState, &TextBuffer)>,
) {
    for ev in events.read() {
        let Ok((sel, buffer)) = q.get(ev.entity) else {
            continue;
        };
        let _ = copy_selection(sel, buffer, &clipboard);
    }
}

pub fn handle_paste(mut events: MessageReader<TerminalPaste>, q: Query<&TerminalSession>) {
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
    mut q: Query<(&mut ScrollState, &mut TerminalScrollFollow, &TextFont)>,
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

/// Synthesize a keypress. Wezterm encodes the key according to the term's
/// current input mode (cursor-key application, kitty keyboard, etc.) and
/// writes the resulting bytes to the PTY.
pub fn handle_key_input(mut events: MessageReader<TerminalKeyInput>, q: Query<&TerminalSession>) {
    for ev in events.read() {
        let Ok(session) = q.get(ev.entity) else {
            continue;
        };
        let _ = session.terminal.lock().key_down(ev.key, ev.mods);
    }
}

/// Forward a POSIX signal to the PTY child's process group. No-op on
/// Windows (process-group signalling has no equivalent).
#[cfg(unix)]
pub fn handle_send_signal(
    mut events: MessageReader<TerminalSendSignal>,
    q: Query<&TerminalSession>,
) {
    for ev in events.read() {
        let Ok(session) = q.get(ev.entity) else {
            continue;
        };
        let pgid = session.pty_master.lock().process_group_leader();
        if let Some(pgid) = pgid {
            // SAFETY: killpg is signal-safe and takes a pid_t + int. We hold
            // no locks here that the signal handler could deadlock against.
            unsafe {
                libc::killpg(pgid as libc::pid_t, ev.signal);
            }
        }
    }
}

#[cfg(not(unix))]
pub fn handle_send_signal(mut events: MessageReader<TerminalSendSignal>) {
    // Drain so the message bus doesn't grow unbounded on Windows.
    for _ in events.read() {}
}

/// Programmatically focus a terminal entity. Mirrors what `on_terminal_added`
/// does on first spawn, but exposed so split-pane hosts can drive focus.
pub fn handle_focus(
    mut events: MessageReader<TerminalFocus>,
    sessions: Query<(), With<TerminalSession>>,
    mut input_focus: ResMut<InputFocus>,
) {
    for ev in events.read() {
        if sessions.get(ev.entity).is_ok() {
            input_focus.set(ev.entity);
        }
    }
}

/// Mirror `Changed<TerminalScrollFollow>` onto the message bus so hosts
/// that prefer events over change-detection queries get notified when the
/// auto-follow state flips.
pub fn emit_scroll_follow_changed(
    q: Query<(Entity, &TerminalScrollFollow), Changed<TerminalScrollFollow>>,
    mut writer: MessageWriter<TerminalScrollFollowChanged>,
    mut last: Local<HashMap<Entity, bool>>,
) {
    for (entity, follow) in q.iter() {
        // `Changed` fires on any field write — including `last_applied_target`,
        // which we update every frame. Filter to actual `stick_to_bottom`
        // transitions so hosts only see meaningful events.
        let prev = last.insert(entity, follow.stick_to_bottom);
        if prev != Some(follow.stick_to_bottom) {
            writer.write(TerminalScrollFollowChanged {
                entity,
                stick_to_bottom: follow.stick_to_bottom,
            });
        }
    }
    last.retain(|e, _| q.get(*e).is_ok());
}
