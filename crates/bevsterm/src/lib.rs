//! Embeddable PTY-backed terminal widget for Bevy.
//!
//! Spawn [`crate::BevyTerminal`] and the `#[require]` cascade brings in
//! everything: the PTY session, VT parser, grid snapshot, color palette, and
//! scrollback config. The shell opens lazily once a non-zero viewport size is
//! available. The plugin handles all VT parsing and PTY I/O internally; the
//! host only needs to position and size the entity.
//!
//! ## Spawning a terminal
//!
//! ```rust,no_run
//! # use bevy::prelude::*;
//! # use bevsterm::prelude::*;
//! // Default shell ($SHELL / powershell.exe), default size, default theme.
//! commands.spawn(BevyTerminal);
//!
//! // Custom shell and working directory.
//! commands.spawn((BevyTerminal, TerminalConfig {
//!     shell: Some("fish".into()),
//!     cwd: Some("/work".into()),
//!     ..default()
//! }));
//! ```
//!
//! ## State the host can read
//!
//! The plugin writes these components every frame — hosts can query them
//! without coupling to plugin internals:
//!
//! - **[`crate::TerminalGridSnapshot`]** — current grid dimensions and cursor
//!   position. Useful for building a status bar or custom cursor overlay.
//! - **[`crate::TerminalShellInfo`]** — title and CWD reported by the shell
//!   via OSC 0/1/2/7. Use this to populate a tab label or breadcrumb.
//! - **[`crate::TerminalBlockState`]** — semantic command blocks from OSC 133
//!   shell integration: each block has a command string, output row range, and
//!   exit code once completed. Subscribe to [`crate::messages::TerminalBlockFinished`]
//!   for completion events.
//! - **[`crate::TerminalScrollFollow`]** — whether the view is pinned to the
//!   bottom. Hosts can toggle this directly or via scroll messages.
//! - **[`crate::TerminalColorPalette`]** — the 16 ANSI colors. Mutate at
//!   runtime to retheme a specific terminal entity.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use bevsterm::prelude::*;
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
pub mod blocks;
pub mod blocks_pick;
pub mod clipboard;
pub mod cursor;
pub mod drain;
pub mod input;
pub mod messages;
pub mod plugin;
pub mod session;
pub mod shell_integration;
pub mod snapshot;
pub mod types;
pub mod viewport;

pub use crate::messages::*;
pub use crate::plugin::{BevyTerminalPlugin, BevyTerminalPlugins};
pub use crate::types::{
    BevyTerminal, BlockStatus, TerminalBlock, TerminalBlockState, TerminalColorPalette,
    TerminalConfig, TerminalGridSnapshot, TerminalInputMode, TerminalScrollFollow,
    TerminalScrollback, TerminalShellInfo, TerminalSession,
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
    // Re-export the bits hosts need to spawn a styled BevyTerminal.
    pub use bevy_instanced_text::{FontConfig, TextViewViewport};
}
