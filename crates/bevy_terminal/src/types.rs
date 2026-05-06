//! Core terminal components.
//!
//! `BevyTerminal` is the only thing the user spawns; `#[require]` cascades
//! the rest. Each component carries a single concern (PTY handle, event
//! channel, grid snapshot, mode flags, scrollback config, theme) so Bevy's
//! change-detection and scheduler do real work.

use std::sync::Arc;

use alacritty_terminal::event::WindowSize;
use alacritty_terminal::event_loop::Notifier;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use bevy::picking::Pickable;
use bevy::prelude::*;

use crate::session::EventProxy;

/// Spawn this on an entity to make it a terminal. The cascade brings in
/// rendering substrate (`TextView` + family), selection state, the
/// session/event/grid/shell/mode/blocks state, the per-entity terminal
/// theme, scrollback config, and `Pickable` for mouse routing.
///
/// The PTY is *not* started here — an `On<Add, BevyTerminal>` observer
/// in `crate::session::spawn` allocates the pty + spawns the event loop
/// thread once defaults are fully cascaded.
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
#[require(
    bevy_text_engine::TextView,
    bevy_text_engine::TextViewState,
    bevy_text_engine::TextViewViewport,
    bevy_text_engine::FontConfig,
    bevy_text_engine::LineStyles,
    bevy_text_engine::BlockList,
    bevy_text_engine::HiddenLines,
    bevy_text_engine::RenderTheme,
    bevy_text_editor::SelectionState,
    bevy_text_editor::EditTheme,
    bevy_text_editor::ScrollConfig,
    TerminalGridSnapshot,
    TerminalShellInfo,
    TerminalInputMode,
    TerminalBlockState,
    TerminalThemeConfig,
    TerminalScrollback,
    Pickable,
)]
pub struct BevyTerminal;

/// Holds the live PTY + Term state. Inserted by the spawn observer; the
/// drain system is the only writer to `terminal` (it takes the lock).
/// Snapshot reads under the same lock and produces `TerminalGridSnapshot`.
///
/// Opaque to reflection — the inner `Arc<FairMutex<Term<_>>>` and
/// `Notifier` carry OS handles and don't make sense to inspect.
#[derive(Component)]
pub struct TerminalSession {
    pub terminal: Arc<FairMutex<Term<EventProxy>>>,
    pub notifier: Notifier,
    /// Last applied window size. Compared against the viewport-derived
    /// (cols, rows) each frame; on change we resize both `Term` and PTY.
    pub size: WindowSize,
}

/// Receiver side of the alacritty `EventLoop` ↔ ECS bridge. Drained each
/// frame in the PTY drain set. Held in its own component (not the session)
/// so other systems can borrow `TerminalSession` mutably without aliasing.
#[derive(Component)]
pub struct TerminalEventChannel {
    pub rx: crossbeam_channel::Receiver<alacritty_terminal::event::Event>,
}

/// Immutable snapshot of the visible grid + cursor + dims. Bumped each
/// frame the `Term` reports damage; the rendering pipeline keys off
/// `version` for change detection.
///
/// `Default` produces an empty 0×0 snapshot; the spawn observer replaces
/// it with the right size on the first sync tick.
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
pub struct TerminalGridSnapshot {
    pub version: u64,
    pub cols: u16,
    pub rows: u16,
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

/// Warp-style command blocks (filled in Phase 5 by the OSC 133 parser).
/// Empty in Phase 1.
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
pub struct TerminalThemeConfig {
    /// ANSI 0..=7 (normal) followed by 8..=15 (bright).
    pub ansi: [Color; 16],
    /// Background tint for the active command block.
    pub block_active: Color,
    /// Background tint for completed blocks.
    pub block_completed: Color,
}

impl Default for TerminalThemeConfig {
    fn default() -> Self {
        // VS Code dark+ palette.
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
