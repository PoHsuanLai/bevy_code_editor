//! # Bevy Terminal
//!
//! Embeddable terminal widget. Spawn a single [`BevyTerminal`] component
//! and `#[require]` cascades the rest — PTY session, grid snapshot, theme,
//! input mode, scrollback. PTY + VT parsing comes from `alacritty_terminal`;
//! rendering from `bevy_text_engine`; selection / clipboard from
//! `bevy_text_editor`.
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

pub mod cursor;
pub mod drain;
pub mod messages;
pub mod plugin;
pub mod session;
pub mod snapshot;
pub mod types;

pub mod prelude {
    pub use crate::messages::*;
    pub use crate::plugin::{
        BevyTerminalPlugin, TerminalActionDispatchSet, TerminalApplyStateSet, TerminalInputSet,
        TerminalPtyDrainSet, TerminalRenderingSet, TerminalSnapshotSet,
    };
    pub use crate::types::{
        BevyTerminal, BlockStatus, TerminalBlock, TerminalBlockState, TerminalGridSnapshot,
        TerminalInputMode, TerminalScrollback, TerminalShellInfo, TerminalThemeConfig,
    };
}
