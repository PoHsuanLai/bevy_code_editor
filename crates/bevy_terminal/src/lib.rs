//! # Bevy Terminal
//!
//! Embeddable terminal widget. Spawn a single [`BevyTerminal`] component
//! and `#[require]` cascades the rest — PTY session, grid snapshot, theme,
//! input mode, scrollback. VT state from `wezterm-term`; PTY allocation
//! from `portable-pty`; rendering from `bevy_text_engine`; selection /
//! clipboard from `bevy_text_editor`.
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use bevy_terminal::prelude::*;
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(BevyTerminalPlugin)
//!     .add_systems(Startup, |mut commands: Commands| {
//!         commands.spawn(BevyTerminal);
//!     })
//!     .run();
//! ```

pub mod backend;
pub mod clipboard;
pub mod cursor;
pub mod drain;
pub mod input;
pub mod messages;
pub mod plugin;
pub mod session;
pub mod snapshot;
pub mod types;
pub mod viewport;

pub mod prelude {
    pub use crate::messages::*;
    pub use crate::plugin::{
        BevyTerminalPlugin, TerminalApplyStateSet, TerminalPtyDrainSet, TerminalSnapshotSet,
    };
    pub use crate::types::{
        BevyTerminal, BlockStatus, TerminalBlock, TerminalBlockState, TerminalGridSnapshot,
        TerminalInputMode, TerminalScrollback, TerminalShellInfo, TerminalColorPalette,
    };
    // Re-export the bits hosts need to spawn a styled BevyTerminal.
    pub use bevy_text_engine::{FontConfig, TextViewViewport};
}
