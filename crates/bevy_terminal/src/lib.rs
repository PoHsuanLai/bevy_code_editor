//! Embeddable PTY-backed terminal widget for Bevy.
//!
//! Spawn [`crate::BevyTerminal`] and the `#[require]` cascade brings in
//! everything: the PTY session, VT parser state, grid snapshot, color palette,
//! input mode flags, and scrollback config. The shell opens lazily once a
//! non-zero viewport size is available, so you never get a stale 80×24 frame.
//!
//! ## Concepts
//!
//! - **[`crate::BevyTerminal`]** — the only component you spawn. Configure the
//!   shell, args, env, and cwd via [`crate::TerminalConfig`] (optional).
//! - **[`crate::TerminalGridSnapshot`]** — the VT grid dimensions and cursor
//!   position, updated each frame from the PTY output.
//! - **[`crate::TerminalColorPalette`]** — the 16 ANSI colors. Override at
//!   spawn time or mutate at runtime for per-terminal theming.
//! - **[`crate::TerminalScrollback`]** — max scrollback line count.
//! - **[`crate::TerminalScrollFollow`]** — whether the view pins to the bottom
//!   as new output arrives.
//!
//! ## Shell integration (OSC 133)
//!
//! When the shell emits OSC 133 sequences, [`crate::TerminalBlockState`] tracks
//! semantic command blocks (prompt → command → output). Subscribe to
//! [`crate::messages::TerminalBlockFinished`] to react to completed commands
//! (e.g. to extract exit codes or capture output).
//!
//! ## What this crate does NOT provide
//!
//! - A title bar or tab UI — hosts build that from [`crate::TerminalShellInfo`]
//! - Find-in-terminal — hosts query the grid snapshot
//! - Terminal multiplexing — spawn one `BevyTerminal` entity per pane
//!
//! ## Quick start
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
    pub use bevy_text_engine::{FontConfig, TextViewViewport};
}
