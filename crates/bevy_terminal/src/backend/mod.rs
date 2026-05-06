//! Wezterm backend facade — the only file in the crate that names
//! `wezterm_term::*` or `termwiz::*`. Everything else imports from
//! `crate::backend` so a future bump, vendor, or fork-publish only
//! touches this module.
//!
//! The facade re-exports the types we use unchanged (small, stable
//! surface), wraps the constructor, and ships a minimal
//! [`DefaultConfig`] / [`AlertChannel`] pair so the rest of the crate
//! gets a default-shaped session with one call to [`make_terminal`].

use std::io::Write;
use std::sync::Arc;

pub use wezterm_term::color::{ColorAttribute, ColorPalette};
pub use wezterm_term::{
    Alert, AlertHandler, CellAttributes, Intensity, SemanticType, SemanticZone, Terminal,
    TerminalConfiguration, TerminalSize, Underline,
};
pub use wezterm_surface::CursorVisibility;

pub use termwiz::input::{KeyCode, KeyboardEncoding, Modifiers as KeyModifiers};

/// Minimum-viable [`TerminalConfiguration`]. Most defaults from upstream
/// are fine; we override `scrollback_size` (so hosts can plumb their own
/// value), enable kitty keyboard + CSI-u key encoding (sane modern
/// defaults that real shells handle gracefully), and supply a default
/// 16-color ANSI palette identical to the one we used under alacritty.
#[derive(Debug)]
pub struct DefaultConfig {
    pub scrollback: usize,
    pub palette: ColorPalette,
    pub kitty_keyboard: bool,
    pub csi_u_keys: bool,
}

impl Default for DefaultConfig {
    fn default() -> Self {
        Self {
            scrollback: 10_000,
            palette: ColorPalette::default(),
            kitty_keyboard: false,
            csi_u_keys: false,
        }
    }
}

impl TerminalConfiguration for DefaultConfig {
    fn color_palette(&self) -> ColorPalette {
        self.palette.clone()
    }
    fn scrollback_size(&self) -> usize {
        self.scrollback
    }
    fn enable_kitty_keyboard(&self) -> bool {
        self.kitty_keyboard
    }
    fn enable_csi_u_key_encoding(&self) -> bool {
        self.csi_u_keys
    }
}

/// `AlertHandler` impl that pushes every received [`Alert`] into a
/// `crossbeam_channel`. Installed on the [`Terminal`] in [`make_terminal`];
/// the ECS drain system reads the receiver each frame and emits the
/// matching Bevy `Message` (bell, title, cwd, …).
pub struct AlertChannel {
    pub tx: crossbeam_channel::Sender<Alert>,
}

impl AlertHandler for AlertChannel {
    fn alert(&mut self, alert: Alert) {
        let _ = self.tx.send(alert);
    }
}

/// Shared-writer shim: lets both wezterm's [`Terminal`] (via the boxed
/// `dyn Write` it consumes) and our ECS systems (via the cloned `Arc`)
/// write to the same underlying PTY input. Inner mutex is `parking_lot`
/// to avoid pulling another mutex flavor.
#[derive(Clone)]
pub struct SharedWriter {
    inner: Arc<parking_lot::Mutex<Box<dyn Write + Send>>>,
}

impl SharedWriter {
    pub fn new(writer: Box<dyn Write + Send>) -> Self {
        Self {
            inner: Arc::new(parking_lot::Mutex::new(writer)),
        }
    }

    pub fn write_bytes(&self, bytes: &[u8]) -> std::io::Result<()> {
        let mut g = self.inner.lock();
        g.write_all(bytes)?;
        g.flush()
    }
}

impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.lock().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.lock().flush()
    }
}

/// Build a [`Terminal`] backed by `writer` (typically the PTY master
/// writer) and an alert channel. Returns the configured terminal, the
/// alert receiver, and a [`SharedWriter`] handle host code can use to
/// write raw bytes without going through the wezterm parser.
pub fn make_terminal(
    size: TerminalSize,
    config: Arc<dyn TerminalConfiguration + Send + Sync>,
    writer: Box<dyn Write + Send>,
) -> (Terminal, crossbeam_channel::Receiver<Alert>, SharedWriter) {
    let shared = SharedWriter::new(writer);
    let (tx, rx) = crossbeam_channel::unbounded::<Alert>();
    let mut terminal = Terminal::new(
        size,
        config,
        "bevy_terminal",
        env!("CARGO_PKG_VERSION"),
        Box::new(shared.clone()),
    );
    terminal.set_notification_handler(Box::new(AlertChannel { tx }));
    (terminal, rx, shared)
}
