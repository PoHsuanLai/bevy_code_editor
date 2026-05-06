//! Drain PTY bytes + wezterm alerts into ECS state.
//!
//! Runs once per frame in `TerminalPtyDrainSet`. Pumps any pending bytes
//! from the reader thread into [`backend::Terminal::advance_bytes`],
//! then drains the alert channel (bell, title, cwd, …) and polls a
//! handful of mode flags + the parser's sequence number so the snapshot
//! system can react to in-band changes.

use bevy::prelude::*;

use crate::backend;
use crate::messages::*;
use crate::types::{
    TerminalEventChannel, TerminalGridSnapshot, TerminalInputMode, TerminalSession,
    TerminalShellInfo,
};

/// Drain pending bytes + alerts and update ECS state.
#[allow(clippy::too_many_arguments)]
pub fn drain_pty_events(
    mut q: Query<(
        Entity,
        &TerminalEventChannel,
        &TerminalSession,
        &mut TerminalShellInfo,
        &mut TerminalInputMode,
        &mut TerminalGridSnapshot,
    )>,
    mut title_w: MessageWriter<TerminalTitleChanged>,
    mut bell_w: MessageWriter<TerminalBellRang>,
    mut cwd_w: MessageWriter<TerminalCwdChanged>,
) {
    for (entity, channel, session, mut shell, mut mode, mut snapshot) in q.iter_mut() {
        // 1. Pump bytes from the reader thread → wezterm parser.
        {
            let mut term = session.terminal.lock();
            while let Ok(bytes) = channel.rx.try_recv() {
                term.advance_bytes(&bytes);
            }
        }

        // 2. Drain async alerts (bell / title / cwd / …).
        while let Ok(alert) = channel.alerts.try_recv() {
            match alert {
                backend::Alert::Bell => {
                    bell_w.write(TerminalBellRang { entity });
                }
                backend::Alert::WindowTitleChanged(title) => {
                    if shell.title != title {
                        shell.title = title.clone();
                        title_w.write(TerminalTitleChanged { entity, title });
                    }
                }
                backend::Alert::CurrentWorkingDirectoryChanged => {
                    let term = session.terminal.lock();
                    if let Some(url) = term.get_current_dir() {
                        let cwd = url.path().to_string();
                        if shell.cwd.as_deref() != Some(&cwd) {
                            shell.cwd = Some(cwd.clone());
                            cwd_w.write(TerminalCwdChanged { entity, cwd });
                        }
                    }
                }
                _ => {}
            }
        }

        // 3. Mirror the public mode flags + sequence number under the lock.
        //
        // wezterm-term encodes keys itself (via `key_down`), so the
        // cursor-key-application + keypad-application + kitty flags that
        // the alacritty path mirrored for our hand-rolled encoder are no
        // longer load-bearing. We keep the fields on `TerminalInputMode`
        // for hosts that want to query them, but only populate the ones
        // wezterm exposes as public getters (alt-screen, bracketed paste,
        // mouse grab, kitty encoding kind).
        let term = session.terminal.lock();
        let new_mode = TerminalInputMode {
            cursor_key_application: false,
            keypad_application: false,
            bracketed_paste: term.bracketed_paste_enabled(),
            alt_screen: term.is_alt_screen_active(),
            mouse_reporting: term.is_mouse_grabbed(),
            kitty_keyboard: matches!(
                term.get_keyboard_encoding(),
                backend::KeyboardEncoding::Kitty(_)
            ),
        };
        if *mode != new_mode {
            *mode = new_mode;
        }
        let seqno = term.current_seqno() as u64;
        if snapshot.version != seqno {
            snapshot.version = seqno;
        }
    }
}
