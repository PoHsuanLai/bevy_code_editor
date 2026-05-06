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
/// start in the `On<Add, BevyTerminal>` observer in `crate::session`.
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
    crate::cursor::TerminalCursorBlink,
    Pickable,
)]
pub struct BevyTerminal;

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
    /// Cursor row in *display* lines (0-based, top of screen).
    pub cursor_row: u16,
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

/// Per-terminal theme: ANSI 16-color palette + UI colors.
///
/// Pure rendering colors come from `bevy_text_engine::RenderTheme`
/// (background, foreground); cursor + selection colors from
/// `bevy_text_editor::EditTheme`. This component carries terminal-specific
/// extras: the 16 ANSI colors plus block backgrounds.
#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct TerminalColorPalette {
    /// ANSI 0..=7 (normal) followed by 8..=15 (bright).
    pub ansi: [Color; 16],
    /// Background tint for the active command block.
    pub block_active: Color,
    /// Background tint for completed blocks.
    pub block_completed: Color,
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
        Self {
            ansi,
            block_active: Color::srgba(0.15, 0.15, 0.18, 0.6),
            block_completed: Color::srgba(0.10, 0.10, 0.13, 0.4),
        }
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
