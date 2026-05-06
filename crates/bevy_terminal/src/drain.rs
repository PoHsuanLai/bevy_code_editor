//! Drain alacritty events from the EventLoop thread into ECS state.
//!
//! Runs once per frame in `TerminalPtyDrainSet`. Reads from the crossbeam
//! receiver, mutates `TerminalShellInfo` / `TerminalInputMode` /
//! `TerminalGridSnapshot.version`, and emits the user-facing messages.

use alacritty_terminal::event::Event as AlacrittyEvent;
use alacritty_terminal::term::TermMode;
use bevy::prelude::*;

use crate::messages::*;
use crate::types::{
    TerminalEventChannel, TerminalGridSnapshot, TerminalInputMode, TerminalSession,
    TerminalShellInfo,
};

/// Drain pending events and update ECS state.
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
    mut exit_w: MessageWriter<TerminalExited>,
) {
    for (entity, channel, session, mut shell, mut mode, mut snapshot) in q.iter_mut() {
        // Drain everything available without blocking. We do not handle
        // PtyWrite / ColorRequest / TextAreaSizeRequest yet — those need
        // round-trip writes back to the PTY, deferred to Phase 4/5.
        while let Ok(ev) = channel.rx.try_recv() {
            match ev {
                AlacrittyEvent::Title(title) => {
                    shell.title = title.clone();
                    title_w.write(TerminalTitleChanged { entity, title });
                }
                AlacrittyEvent::ResetTitle => {
                    shell.title.clear();
                    title_w.write(TerminalTitleChanged {
                        entity,
                        title: String::new(),
                    });
                }
                AlacrittyEvent::Bell => {
                    bell_w.write(TerminalBellRang { entity });
                }
                AlacrittyEvent::ChildExit(status) => {
                    exit_w.write(TerminalExited {
                        entity,
                        status: Some(ExitStatusReflect(status)),
                    });
                }
                AlacrittyEvent::Wakeup
                | AlacrittyEvent::MouseCursorDirty
                | AlacrittyEvent::CursorBlinkingChange => {
                    // Mark the snapshot stale so the snapshot system rebuilds.
                    snapshot.version = snapshot.version.wrapping_add(1);
                }
                AlacrittyEvent::Exit
                | AlacrittyEvent::PtyWrite(_)
                | AlacrittyEvent::ClipboardStore(_, _)
                | AlacrittyEvent::ClipboardLoad(_, _)
                | AlacrittyEvent::ColorRequest(_, _)
                | AlacrittyEvent::TextAreaSizeRequest(_) => {
                    // Phase 4/5 hooks — handled when clipboard + osc are wired.
                }
            }
        }

        // After draining events, sync mode flags by peeking at the term.
        // Lock-grab is cheap (FairMutex, brief). Doing this here means a
        // single place in the pipeline owns "terminal state → ECS"; the
        // snapshot system later in the same set takes the lock again to
        // build the cell snapshot. Two locks per frame; acceptable.
        let term = session.terminal.lock();
        let term_mode = *term.mode();
        let new_mode = TerminalInputMode {
            cursor_key_application: term_mode.contains(TermMode::APP_CURSOR),
            keypad_application: term_mode.contains(TermMode::APP_KEYPAD),
            bracketed_paste: term_mode.contains(TermMode::BRACKETED_PASTE),
            alt_screen: term_mode.contains(TermMode::ALT_SCREEN),
            mouse_reporting: term_mode.contains(TermMode::MOUSE_MODE),
            kitty_keyboard: term_mode.contains(TermMode::REPORT_ASSOCIATED_TEXT)
                || term_mode.contains(TermMode::REPORT_EVENT_TYPES)
                || term_mode.contains(TermMode::DISAMBIGUATE_ESC_CODES),
        };
        if *mode != new_mode {
            *mode = new_mode;
        }
    }
}

