//! PTY session lifecycle: spawn (`On<Add>`), shutdown (`On<Remove>`).

use std::io::Read;
use std::sync::Arc;
use std::thread::JoinHandle;

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use parking_lot::Mutex;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

use crate::backend;
use crate::messages::TerminalReady;
use crate::types::{
    BevyTerminal, TerminalEventChannel, TerminalScrollback, TerminalSession,
};

const DEFAULT_COLS: u16 = 80;
const DEFAULT_ROWS: u16 = 24;
const DEFAULT_CELL_W: u16 = 8;
const DEFAULT_CELL_H: u16 = 16;

#[derive(Resource, Default)]
pub struct TerminalEventLoopRegistry {
    pub handles: Vec<JoinHandle<()>>,
}

pub fn on_terminal_added(
    trigger: On<Add, BevyTerminal>,
    scrollbacks: Query<&TerminalScrollback>,
    mut commands: Commands,
    mut registry: ResMut<TerminalEventLoopRegistry>,
    mut input_focus: ResMut<InputFocus>,
    mut ready_w: MessageWriter<TerminalReady>,
) {
    let entity = trigger.entity;
    let scrollback = scrollbacks
        .get(entity)
        .cloned()
        .unwrap_or_default();

    match build_session(scrollback.max_lines) {
        Ok((session, channel, handle)) => {
            let cols = session.size.cols as u16;
            let rows = session.size.rows as u16;
            registry.handles.push(handle);
            commands.entity(entity).insert((session, channel));
            if input_focus.get().is_none() {
                input_focus.set(entity);
            }
            ready_w.write(TerminalReady { entity, cols, rows });
        }
        Err(err) => {
            error!("bevy_terminal: failed to spawn PTY: {err}");
        }
    }
}

fn build_session(
    scrollback_lines: usize,
) -> std::io::Result<(TerminalSession, TerminalEventChannel, JoinHandle<()>)> {
    let pty_system = native_pty_system();
    let pty_pair = pty_system
        .openpty(PtySize {
            rows: DEFAULT_ROWS,
            cols: DEFAULT_COLS,
            pixel_width: DEFAULT_COLS * DEFAULT_CELL_W,
            pixel_height: DEFAULT_ROWS * DEFAULT_CELL_H,
        })
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

    let writer = pty_pair
        .master
        .take_writer()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

    let size = backend::TerminalSize {
        rows: DEFAULT_ROWS as usize,
        cols: DEFAULT_COLS as usize,
        pixel_width: (DEFAULT_COLS * DEFAULT_CELL_W) as usize,
        pixel_height: (DEFAULT_ROWS * DEFAULT_CELL_H) as usize,
        dpi: 0,
    };

    let config = Arc::new(backend::DefaultConfig {
        scrollback: scrollback_lines,
        ..Default::default()
    }) as Arc<dyn backend::TerminalConfiguration + Send + Sync>;

    let (terminal, alerts_rx, pty_input) = backend::make_terminal(size, config, writer);
    let terminal = Arc::new(Mutex::new(terminal));

    let cmd = default_shell_command();
    pty_pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

    let mut reader = pty_pair
        .master
        .try_clone_reader()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

    let (tx, rx) = crossbeam_channel::unbounded::<Vec<u8>>();
    let handle = std::thread::Builder::new()
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
        })?;

    let pty_master = Arc::new(Mutex::new(pty_pair.master));

    Ok((
        TerminalSession {
            terminal,
            pty_master,
            pty_input,
            size,
        },
        TerminalEventChannel { rx, alerts: alerts_rx },
        handle,
    ))
}

fn default_shell_command() -> CommandBuilder {
    if cfg!(windows) {
        CommandBuilder::new("powershell.exe")
    } else {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        let mut cmd = CommandBuilder::new(shell);
        if let Ok(home) = std::env::var("HOME") {
            cmd.cwd(home);
        }
        cmd
    }
}

pub fn on_terminal_removed(
    _trigger: On<Remove, BevyTerminal>,
    _sessions: Query<&TerminalSession>,
) {
}
