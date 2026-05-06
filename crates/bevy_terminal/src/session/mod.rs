//! PTY session lifecycle: spawn (`On<Add>`), drain (per-frame system),
//! shutdown (`On<Remove>`).
//!
//! `alacritty_terminal` runs the PTY I/O loop on its own thread; events
//! come back to ECS through a crossbeam channel via [`EventProxy`]. The
//! drain system in `crate::plugin` pulls events and updates ECS components.

use std::sync::Arc;
use std::thread::JoinHandle;

use alacritty_terminal::event::{Event as AlacrittyEvent, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, Notifier};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::tty;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};

use crate::types::{
    BevyTerminal, TerminalEventChannel, TerminalScrollback, TerminalSession,
};

/// Initial PTY dimensions used when the entity is spawned before its
/// viewport / font has settled. The viewport-resize observer (Phase 4)
/// resizes both Term and PTY to the actual visible (cols, rows).
const DEFAULT_COLS: u16 = 80;
const DEFAULT_ROWS: u16 = 24;
/// Reasonable cell-size hint for the PTY. The shell doesn't truly care
/// about pixels — it reads cols/rows.
const DEFAULT_CELL_W: u16 = 8;
const DEFAULT_CELL_H: u16 = 16;

/// Cloneable bridge implementing alacritty's `EventListener`. Both
/// `Term<Self>` and the `EventLoop` thread hold a copy; events arrive at
/// the same channel.
#[derive(Clone)]
pub struct EventProxy {
    tx: Sender<AlacrittyEvent>,
}

impl EventProxy {
    pub fn new(tx: Sender<AlacrittyEvent>) -> Self {
        Self { tx }
    }
}

impl EventListener for EventProxy {
    fn send_event(&self, event: AlacrittyEvent) {
        // Channel send only fails when the receiver is dropped — meaning
        // the entity has been despawned. Drop the event silently.
        let _ = self.tx.send(event);
    }
}

/// Tiny `Dimensions` shim so we can construct a `Term` before its grid
/// exists.
struct Dims {
    cols: usize,
    rows: usize,
}

impl Dimensions for Dims {
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

/// Tracker for spawned event-loop threads — held in a non-`Send` registry
/// resource so they can be joined on app shutdown without leaking.
#[derive(Resource, Default)]
pub struct TerminalEventLoopRegistry {
    pub handles: Vec<JoinHandle<()>>,
}

/// Observer wired in `BevyTerminalPlugin`: spawn a PTY + EventLoop + Term
/// for any entity that gains a `BevyTerminal` marker. Also seeds
/// `InputFocus` if nothing else has it yet — handy for single-terminal
/// apps that don't need to click before typing.
pub fn on_terminal_added(
    trigger: On<Add, BevyTerminal>,
    scrollbacks: Query<&TerminalScrollback>,
    mut commands: Commands,
    mut registry: ResMut<TerminalEventLoopRegistry>,
    mut input_focus: ResMut<InputFocus>,
) {
    let entity = trigger.entity;
    let scrollback = scrollbacks
        .get(entity)
        .cloned()
        .unwrap_or_default();

    match build_session(scrollback.max_lines) {
        Ok((session, channel, handle)) => {
            registry.handles.push(handle);
            commands.entity(entity).insert((session, channel));
            if input_focus.get().is_none() {
                input_focus.set(entity);
            }
        }
        Err(err) => {
            error!("bevy_terminal: failed to spawn PTY: {err}");
        }
    }
}

/// All-in-one session builder: opens the PTY, constructs `Term`, spawns
/// the alacritty `EventLoop` thread, returns the ECS components plus
/// the join handle (for clean shutdown).
fn build_session(
    scrollback_lines: usize,
) -> std::io::Result<(TerminalSession, TerminalEventChannel, JoinHandle<()>)> {
    let (tx, rx): (Sender<AlacrittyEvent>, Receiver<AlacrittyEvent>) =
        crossbeam_channel::unbounded();
    let proxy = EventProxy::new(tx);

    let size = WindowSize {
        num_lines: DEFAULT_ROWS,
        num_cols: DEFAULT_COLS,
        cell_width: DEFAULT_CELL_W,
        cell_height: DEFAULT_CELL_H,
    };

    let pty = tty::new(&tty::Options::default(), size, /* window_id */ 0)?;

    let mut term_cfg = Config::default();
    term_cfg.scrolling_history = scrollback_lines;

    let dims = Dims {
        cols: DEFAULT_COLS as usize,
        rows: DEFAULT_ROWS as usize,
    };
    let terminal = Term::new(term_cfg, &dims, proxy.clone());
    let terminal = Arc::new(FairMutex::new(terminal));

    let event_loop = EventLoop::new(
        terminal.clone(),
        proxy,
        pty,
        /* drain_on_exit */ false,
        /* ref_test */ false,
    )?;
    let notifier = Notifier(event_loop.channel());
    let handle = event_loop.spawn();

    // EventLoop::spawn returns `JoinHandle<(EventLoop, State)>`; we don't
    // care about the inner tuple, just that we can join the thread on
    // shutdown. Wrap to a unit-returning handle.
    let handle = std::thread::spawn(move || {
        let _ = handle.join();
    });

    Ok((
        TerminalSession {
            terminal,
            notifier,
            size,
        },
        TerminalEventChannel { rx },
        handle,
    ))
}

/// Observer for cleanup when the terminal entity is removed/despawned.
///
/// V1 leaves the EventLoop thread to be reaped at process exit (its sender
/// errors when our crossbeam receiver is dropped, and `EventLoop` exits its
/// poll loop on the next PTY error). Wire `Msg::Shutdown` here when we add
/// a shutdown sender to `TerminalSession`.
pub fn on_terminal_removed(
    _trigger: On<Remove, BevyTerminal>,
    _sessions: Query<&TerminalSession>,
) {
}
