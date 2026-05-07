//! Core terminal components.
//!
//! `BevyTerminal` is the only thing the user spawns; `#[require]` cascades
//! the rest. Each component carries a single concern (PTY handle, event
//! channel, grid snapshot, mode flags, scrollback config, theme) so Bevy's
//! change-detection and scheduler do real work.

use std::sync::Arc;

use bevy::picking::Pickable;
use bevy::prelude::*;
use parking_lot::Mutex;

use crate::backend;

/// Spawn this on an entity to make it a terminal; the `#[require]`
/// cascade brings in rendering substrate, selection state, terminal
/// state, theme, and `Pickable` for mouse routing. PTY + child shell
/// open lazily once `TextViewViewport` and `FontConfig` produce a
/// non-zero (cols, rows), so the shell never renders a stale 80×24
/// frame. Configure shell / argv / env / cwd via [`TerminalConfig`].
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
#[require(
    bevy_text_engine::TextView,
    bevy_text_engine::TextBuffer,
    bevy_text_engine::ScrollState,
    bevy_text_engine::ContentMetrics,
    bevy_text_engine::TextViewViewport,
    bevy_text_engine::FontConfig,
    bevy_text_engine::LineStyles,
    bevy_text_engine::HiddenLines,
    bevy_text_engine::RenderTheme,
    bevy_text_engine::BlockDecorTheme,
    bevy_text_editor::SelectionState,
    bevy_text_editor::EditTheme,
    bevy_text_editor::ScrollConfig,
    TerminalGridSnapshot,
    TerminalShellInfo,
    TerminalInputMode,
    TerminalBlockState,
    TerminalColorPalette,
    TerminalScrollback,
    TerminalScrollFollow,
    crate::cursor::TerminalCursorBlink,
    Pickable,
)]
pub struct BevyTerminal;

/// Per-spawn shell configuration. Optional — omit and the session uses
/// `$SHELL` (or `powershell.exe` on Windows), no extra args, the user's
/// `$HOME` as cwd, and no environment overrides on top of the parent's.
///
/// Insert alongside `BevyTerminal` to override any subset of these:
///
/// ```rust,ignore
/// commands.spawn((
///     BevyTerminal,
///     TerminalConfig {
///         shell: Some("bash".into()),
///         args: vec!["-l".into()],
///         env: vec![("TERM".into(), "xterm-256color".into())],
///         cwd: Some("/work".into()),
///     },
///     font,
///     viewport,
/// ));
/// ```
///
/// Read once when the PTY is opened. Mutating fields after the session
/// exists has no effect — despawn and respawn instead.
#[derive(Component, Clone, Debug, Default, Reflect)]
#[reflect(Component, Default)]
pub struct TerminalConfig {
    /// Program to launch. `None` → fall back to `$SHELL` (Unix) /
    /// `powershell.exe` (Windows).
    pub shell: Option<String>,
    /// Arguments passed to the shell.
    pub args: Vec<String>,
    /// Environment overrides layered on top of the parent process's env.
    /// Stored as a `Vec` rather than a `HashMap` to keep `Reflect` happy.
    pub env: Vec<(String, String)>,
    /// Working directory. `None` → `$HOME` on Unix, parent's cwd on Windows.
    pub cwd: Option<String>,
}

#[derive(Component)]
pub struct TerminalSession {
    pub terminal: Arc<Mutex<backend::Terminal>>,
    pub pty_master: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
    pub pty_input: backend::SharedWriter,
    pub size: backend::TerminalSize,
}

#[derive(Component)]
pub struct TerminalEventChannel {
    pub rx: crossbeam_channel::Receiver<Vec<u8>>,
    pub alerts: crossbeam_channel::Receiver<backend::Alert>,
}

/// Snapshot of the visible grid metadata: dims + cursor + version. The
/// actual cell text/styles flow through `LineStyles` (engine signal-
/// component); the cursor stays here so input + overlay code can read
/// without touching the term lock.
///
/// `Default` produces an empty 0×0 snapshot; the spawn observer replaces
/// it with the right size on the first sync tick.
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
pub struct TerminalGridSnapshot {
    pub version: u64,
    pub cols: u16,
    pub rows: u16,
    /// Cursor row as a buffer-line index (0 = top of scrollback, growing
    /// downward). The visible window is the last `rows` lines, so when
    /// scrollback is non-empty `cursor_row >= total_lines - rows`.
    pub cursor_row: u32,
    /// Cursor column (0-based).
    pub cursor_col: u16,
    /// `true` when the cursor is hidden (DECTCEM off).
    pub cursor_hidden: bool,
}

/// Shell info reported by the PTY child + OSC 0/1/2/7 escape sequences.
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
pub struct TerminalShellInfo {
    pub title: String,
    pub cwd: Option<String>,
}

/// Mode flags driven by the alacritty `Term`'s `TermMode`. Mirrored into
/// ECS so input observers can read them without taking the term lock.
#[derive(Component, Default, Clone, Copy, PartialEq, Eq, Reflect)]
#[reflect(Component, Default, PartialEq)]
pub struct TerminalInputMode {
    pub cursor_key_application: bool,
    pub keypad_application: bool,
    pub bracketed_paste: bool,
    pub alt_screen: bool,
    pub mouse_reporting: bool,
    pub kitty_keyboard: bool,
}

/// Warp-style command blocks parsed from OSC 133. Empty without shell-integration.
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
pub struct TerminalBlockState {
    pub blocks: Vec<TerminalBlock>,
    pub current_block: Option<usize>,
}

#[derive(Clone, Debug, Default, Reflect)]
pub struct TerminalBlock {
    pub id: u64,
    pub status: BlockStatus,
    pub exit_code: Option<i32>,
    pub prompt_row: i64,
    pub output_row: i64,
    pub end_row: i64,
    pub command_text: String,
}

#[derive(Clone, Copy, Debug, Default, Reflect, PartialEq, Eq)]
pub enum BlockStatus {
    #[default]
    Pending,
    Running,
    Completed,
}

/// Per-terminal theme: ANSI 16-color palette.
///
/// Pure rendering colors come from `bevy_text_engine::RenderTheme`
/// (background, foreground); cursor + selection colors from
/// `bevy_text_editor::EditTheme`. This component carries the
/// terminal-specific 16 ANSI colors used by the grid snapshot.
#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct TerminalColorPalette {
    /// ANSI 0..=7 (normal) followed by 8..=15 (bright).
    pub ansi: [Color; 16],
}

impl Default for TerminalColorPalette {
    fn default() -> Self {
        // VS Code "Dark+" palette. Hosts that want a different look should
        // override the component on spawn or mutate it at runtime.
        let ansi = [
            Color::srgb(0.000, 0.000, 0.000), // 0 black
            Color::srgb(0.804, 0.000, 0.000), // 1 red
            Color::srgb(0.000, 0.804, 0.000), // 2 green
            Color::srgb(0.804, 0.804, 0.000), // 3 yellow
            Color::srgb(0.000, 0.000, 0.804), // 4 blue
            Color::srgb(0.804, 0.000, 0.804), // 5 magenta
            Color::srgb(0.000, 0.804, 0.804), // 6 cyan
            Color::srgb(0.898, 0.898, 0.898), // 7 white
            Color::srgb(0.494, 0.494, 0.494), // 8 bright black
            Color::srgb(1.000, 0.000, 0.000), // 9 bright red
            Color::srgb(0.000, 1.000, 0.000), // 10 bright green
            Color::srgb(1.000, 1.000, 0.000), // 11 bright yellow
            Color::srgb(0.357, 0.502, 1.000), // 12 bright blue
            Color::srgb(1.000, 0.000, 1.000), // 13 bright magenta
            Color::srgb(0.000, 1.000, 1.000), // 14 bright cyan
            Color::srgb(1.000, 1.000, 1.000), // 15 bright white
        ];
        Self { ansi }
    }
}

/// Scrollback configuration. Maps to `alacritty_terminal::term::Config::scrolling_history`.
#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct TerminalScrollback {
    /// Maximum number of lines kept in history.
    pub max_lines: usize,
}

impl Default for TerminalScrollback {
    fn default() -> Self {
        Self { max_lines: 10_000 }
    }
}

/// Bottom-follow state. When `stick_to_bottom` is `true`, `sync_grid_snapshot`
/// pins `ScrollState` to the latest output so new lines stay visible. The wheel
/// observer flips it `false` when the user scrolls away from the bottom; the
/// same system flips it back `true` when they wheel within one row of the
/// bottom. Hosts can drive it directly (toggle button) or via the
/// [`crate::messages::TerminalScrollToBottom`] /
/// [`crate::messages::TerminalScrollToTop`] /
/// [`crate::messages::TerminalScrollTo`] messages.
#[derive(Component, Clone, Copy, Debug, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct TerminalScrollFollow {
    pub stick_to_bottom: bool,
    /// `target_scroll_offset` we last wrote — internal book-keeping the system
    /// uses to detect wheel input. Hosts should leave this alone.
    pub last_applied_target: f32,
}

impl Default for TerminalScrollFollow {
    fn default() -> Self {
        Self {
            stick_to_bottom: true,
            last_applied_target: 0.0,
        }
    }
}
