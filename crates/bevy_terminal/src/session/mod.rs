//! PTY session lifecycle: deferred open once viewport is known, kill +
//! thread join on entity removal.

use std::io::Read;
use std::sync::Arc;
use std::thread::JoinHandle;

use bevy::input_focus::InputFocus;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy_text_engine::{FontConfig, TextViewViewport};
use parking_lot::Mutex;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, PtySize};

use crate::backend;
use crate::messages::{TerminalReady, TerminalSpawnFailed};
use crate::types::{
    BevyTerminal, TerminalConfig, TerminalEventChannel, TerminalScrollback, TerminalSession,
};
use crate::viewport::{cells_from_viewport, MIN_COLS, MIN_ROWS};

/// Per-entity book-keeping for the reader thread + child killer so the
/// removal observer can shut both down deterministically.
pub(crate) struct ReaderHandle {
    pub join: JoinHandle<()>,
    pub killer: Box<dyn ChildKiller + Send + Sync>,
}

/// Tracks per-entity PTY reader threads and child killers so the removal
/// observer can shut them down deterministically when a `BevyTerminal` entity
/// is despawned.
#[derive(Resource, Default)]
pub struct TerminalEventLoopRegistry {
    pub(crate) handles: HashMap<Entity, ReaderHandle>,
}

/// Open the PTY for any `BevyTerminal` entity that has acquired a usable
/// viewport + font but doesn't yet have a `TerminalSession`. Runs every
/// frame in `TerminalApplyStateSet` and short-circuits once the session
/// exists. Replaces the old eager `On<Add>` observer that opened the PTY
/// at a hardcoded 80×24, causing visible reflow when the real size landed.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn open_pending_sessions(
    pending: Query<
        (
            Entity,
            &TextViewViewport,
            &FontConfig,
            Option<&TerminalConfig>,
            Option<&TerminalScrollback>,
        ),
        (With<BevyTerminal>, Without<TerminalSession>),
    >,
    windows: Query<&bevy::window::Window, With<bevy::window::PrimaryWindow>>,
    mut commands: Commands,
    mut registry: ResMut<TerminalEventLoopRegistry>,
    mut input_focus: ResMut<InputFocus>,
    mut ready_w: MessageWriter<TerminalReady>,
    mut failed_w: MessageWriter<TerminalSpawnFailed>,
) {
    let scale = windows
        .single()
        .map(|w| w.scale_factor() as u32)
        .unwrap_or(1)
        .max(1);

    for (entity, viewport, font, config, scrollback) in &pending {
        let Some((cols, rows)) = cells_from_viewport(viewport, font) else {
            continue;
        };
        if cols < MIN_COLS || rows < MIN_ROWS {
            continue;
        }
        let cell_w = font.char_width.round().max(1.0) as u16;
        let cell_h = font.line_height.round().max(1.0) as u16;
        let scrollback_lines = scrollback.cloned().unwrap_or_default().max_lines;
        let cmd = build_command(config);

        match build_session(cols, rows, cell_w, cell_h, scale, scrollback_lines, cmd) {
            Ok((session, channel, reader_handle)) => {
                registry.handles.insert(entity, reader_handle);
                commands.entity(entity).insert((session, channel));
                if input_focus.get().is_none() {
                    input_focus.set(entity);
                }
                ready_w.write(TerminalReady { entity, cols, rows });
            }
            Err(err) => {
                let error = err.to_string();
                error!("bevy_terminal: PTY spawn failed for {entity:?}: {error}");
                failed_w.write(TerminalSpawnFailed { entity, error });
                commands.entity(entity).despawn();
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_session(
    cols: u16,
    rows: u16,
    cell_w: u16,
    cell_h: u16,
    scale: u32,
    scrollback_lines: usize,
    cmd: CommandBuilder,
) -> std::io::Result<(TerminalSession, TerminalEventChannel, ReaderHandle)> {
    let pty_system = native_pty_system();
    let pty_pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: cols * cell_w,
            pixel_height: rows * cell_h,
        })
        .map_err(io_error)?;

    let writer = pty_pair.master.take_writer().map_err(io_error)?;

    let size = backend::TerminalSize {
        rows: rows as usize,
        cols: cols as usize,
        pixel_width: (cols * cell_w) as usize,
        pixel_height: (rows * cell_h) as usize,
        dpi: scale,
    };

    let config = Arc::new(backend::DefaultConfig {
        scrollback: scrollback_lines,
        ..Default::default()
    }) as Arc<dyn backend::TerminalConfiguration + Send + Sync>;

    let (terminal, alerts_rx, pty_input) = backend::make_terminal(size, config, writer);
    let terminal = Arc::new(Mutex::new(terminal));

    let child = pty_pair.slave.spawn_command(cmd).map_err(io_error)?;
    let killer = child.clone_killer();

    let mut reader = pty_pair.master.try_clone_reader().map_err(io_error)?;

    let (tx, rx) = crossbeam_channel::unbounded::<Vec<u8>>();
    let join = std::thread::Builder::new()
        .name("bevy_terminal_pty_reader".into())
        .spawn(move || {
            let mut buf = vec![0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            // Drop our handle on the child so its FDs close once the killer
            // (held by the registry) drops too.
            drop(child);
        })?;

    let pty_master = Arc::new(Mutex::new(pty_pair.master));

    Ok((
        TerminalSession {
            terminal,
            pty_master,
            pty_input,
            size,
        },
        TerminalEventChannel {
            rx,
            alerts: alerts_rx,
        },
        ReaderHandle { join, killer },
    ))
}

fn io_error<E: std::fmt::Display>(e: E) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
}

fn build_command(config: Option<&TerminalConfig>) -> CommandBuilder {
    let shell = config
        .and_then(|c| c.shell.clone())
        .unwrap_or_else(default_shell);
    let mut cmd = CommandBuilder::new(shell);

    if let Some(c) = config {
        cmd.args(&c.args);
        for (k, v) in &c.env {
            cmd.env(k, v);
        }
    }

    let cwd = config
        .and_then(|c| c.cwd.clone())
        .or_else(|| std::env::var("HOME").ok());
    if let Some(cwd) = cwd {
        cmd.cwd(cwd);
    }
    cmd
}

fn default_shell() -> String {
    if cfg!(windows) {
        "powershell.exe".into()
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into())
    }
}

/// Kill the child shell and join the reader thread when a `BevyTerminal`
/// entity is removed. The killer is sendable so calling it from the main
/// thread unblocks the reader's `read()` and the join finishes promptly.
pub fn on_terminal_removed(
    trigger: On<Remove, BevyTerminal>,
    mut registry: ResMut<TerminalEventLoopRegistry>,
) {
    let entity = trigger.entity;
    let Some(mut handle) = registry.handles.remove(&entity) else {
        return;
    };
    if let Err(e) = handle.killer.kill() {
        // Already exited — fine. Anything else is rare and worth logging.
        if e.kind() != std::io::ErrorKind::InvalidInput
            && e.kind() != std::io::ErrorKind::NotFound
        {
            warn!("bevy_terminal: kill failed for {entity:?}: {e}");
        }
    }
    if let Err(panic) = handle.join.join() {
        warn!("bevy_terminal: reader thread panicked for {entity:?}: {panic:?}");
    }
}
