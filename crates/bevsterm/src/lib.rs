//! Embeddable terminal widget for Bevy.
//!
//! ## Native usage (PTY-backed shell)
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use bevsterm::prelude::*;
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(BevyTerminalPlugins)   // renderer + PTY backend
//!     .add_systems(Startup, |mut commands: Commands| {
//!         commands.spawn(BevyTerminal);
//!     })
//!     .run();
//! ```
//!
//! ## WASM / custom IO
//!
//! Add only [`BevyTerminalPlugin`] (renderer), then insert
//! [`TerminalSession`] and [`TerminalEventChannel`] on the entity yourself
//! with channels backed by a WebSocket or other transport:
//!
//! ```rust,no_run
//! # use bevy::prelude::*;
//! # use bevsterm::prelude::*;
//! # use bevsterm::{TerminalSession, TerminalEventChannel};
//! # fn setup(mut commands: Commands) {
//! let (tx, rx) = crossbeam_channel::unbounded();
//! let (alerts_tx, alerts_rx) = crossbeam_channel::unbounded();
//! let writer = /* Box<dyn std::io::Write + Send> backed by WebSocket */
//! # Box::new(std::io::sink());
//! let size = bevsterm::backend::TerminalSize { rows: 24, cols: 80, ..Default::default() };
//! let config = std::sync::Arc::new(bevsterm::backend::DefaultConfig::default());
//! let (terminal, _, pty_input) = bevsterm::backend::make_terminal(size, config, writer);
//! let terminal = std::sync::Arc::new(parking_lot::Mutex::new(terminal));
//! commands.spawn((
//!     BevyTerminal,
//!     TerminalSession { terminal, pty_input, size },
//!     TerminalEventChannel { rx, alerts: alerts_rx },
//! ));
//! # }
//! ```
//!
//! ## State the host can read
//!
//! - **[`TerminalGridSnapshot`]** — grid dimensions and cursor position.
//! - **[`TerminalShellInfo`]** — title and CWD from OSC 0/1/2/7.
//! - **[`TerminalBlockState`]** — semantic command blocks from OSC 133.
//! - **[`TerminalScrollFollow`]** — whether the view is pinned to the bottom.
//! - **[`TerminalColorPalette`]** — the 16 ANSI colors; mutate to retheme.

pub mod backend;
pub mod blocks;
pub mod blocks_pick;
pub mod clipboard;
pub mod cursor;
pub mod drain;
pub mod input;
pub mod messages;
pub mod plugin;
#[cfg(feature = "pty")]
pub mod session;
pub mod shell_integration;
pub mod snapshot;
pub mod types;
pub mod viewport;

pub use crate::messages::*;
pub use crate::plugin::{BevyTerminalPlugin, BevyTerminalPlugins};
#[cfg(feature = "pty")]
pub use crate::session::BevyTerminalPtyPlugin;
pub use crate::types::{
    BevyTerminal, BlockStatus, TerminalBlock, TerminalBlockState, TerminalColorPalette,
    TerminalConfig, TerminalEventChannel, TerminalGridSnapshot, TerminalInputMode,
    TerminalScrollFollow, TerminalScrollback, TerminalSession, TerminalShellInfo,
};

pub mod prelude {
    pub use crate::messages::*;
    pub use crate::plugin::{
        BevyTerminalPlugin, BevyTerminalPlugins, TerminalApplyStateSet, TerminalPtyDrainSet,
        TerminalSnapshotSet,
    };
    pub use crate::types::{
        BevyTerminal, BlockStatus, TerminalBlock, TerminalBlockState, TerminalColorPalette,
        TerminalConfig, TerminalGridSnapshot, TerminalInputMode, TerminalScrollFollow,
        TerminalScrollback, TerminalShellInfo,
    };
    pub use bevy_instanced_text::{MonoCellWidth, MonoFontFaces};
    pub use bevy::text::TextFont;
}
