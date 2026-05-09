//! Editable text widget for Bevy, built on top of `bevy_instanced_text`.
//!
//! This crate adds interactivity to any [`bevy_instanced_text::TextView`] entity.
//! It does not know what the text *means* — no syntax, no LSP, no file I/O.
//! It owns the mechanics of text editing: where the cursor goes, what happens
//! when you type, how history is recorded.
//!
//! ## Concepts
//!
//! Two plugins, composable independently:
//!
//! - **[`InstancedTextInteractionPlugin`]** — pointer interaction only: click-to-place
//!   cursor, drag-select, scroll wheel, double/triple-click word/line select,
//!   and copy. No keyboard input. Suitable for read-only views that need
//!   text selection (log viewers, output panels).
//!
//! - **[`InstancedTextEditPlugin`]** — everything in `InstancedTextInteractionPlugin` plus
//!   typed-character input, full edit history, undo/redo, clipboard cut/paste,
//!   and keyboard shortcuts dispatched as [`editing_events`] messages.
//!
//! ## Dispatch model
//!
//! All editing operations are expressed as `*Requested` messages (e.g.
//! [`DeleteBackwardRequested`], [`MoveCursorLeftRequested`]). Systems send a
//! message; handler systems in this crate apply it to the focused
//! [`TextEditor`] entity. Hosts can send the same messages programmatically to
//! drive the editor without simulating keystrokes.
//!
//! ## Key components
//!
//! - **[`TextEditor`]** — marker that opts an entity into editable-text
//!   handling. Cascades [`CursorState`], [`SelectionState`],
//!   [`EditHistoryState`], and [`EditTheme`].
//! - **[`CursorState`]** — cursor position as a rope char offset.
//! - **[`SelectionState`]** — multi-cursor selection ranges.
//! - **[`EditHistoryState`]** — undo/redo stack.
//! - **[`CursorSettings`]** — caret shape, blink rate, key-repeat timing.
//! - **[`ClipboardResource`]** — swappable clipboard backend
//!   ([`SystemClipboard`] by default when the `arboard` feature is on).
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use bevy_instanced_text::prelude::*;
//! use bevy_instanced_text_edit::InstancedTextEditPlugin;
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(InstancedTextPlugins)
//!     .add_plugins(InstancedTextEditPlugin::default())
//!     .add_systems(Startup, |mut commands: Commands| {
//!         commands.spawn((
//!             bevy_instanced_text_edit::TextEditor,
//!             TextBuffer::from_str("edit me"),
//!             TextViewViewport::default(),
//!             FontConfig::default(),
//!         ));
//!     })
//!     .run();
//! ```

pub mod anchor;
pub mod clipboard;
pub mod components;
pub mod cursor_movement;
pub mod cursor_settings;
pub mod edit;
pub mod editing_events;
pub mod handlers;
pub mod history;
pub mod interaction;
pub mod key_repeat;
pub mod picking;
pub mod plugin;
pub mod prelude;
pub mod selection;
pub mod state;
pub mod theme;
pub mod typing;

pub use anchor::{Anchor, AnchorBias, AnchorSet, TextEdit};
#[cfg(feature = "arboard")]
pub use clipboard::SystemClipboard;
pub use clipboard::{ClipboardProvider, ClipboardResource, NullClipboard};
pub use components::{ScrollConfig, TextViewDragState};
pub use cursor_settings::{
    caret_overlay, cursor_blink_visible, BlinkPhase, CursorSettings, CursorStyle,
};
pub use edit::{point_at_byte, EditOutcome};
pub use editing_events::*;
pub use history::{EditHistory, EditKind, EditOperation, EditTransaction};
pub use interaction::{copy_selection, screen_to_char_pos, InteractionSettings};
pub use key_repeat::{KeyRepeatSettings, KeyRepeatState};
pub use plugin::{EditEmitSet, InstancedTextEditPlugin, InstancedTextInteractionPlugin};
pub use selection::{Selection, SelectionCollection, SelectionMode, DEFAULT_SEMANTIC_ESCAPE_CHARS};
pub use state::{
    CursorState, EditDelta, EditHistoryState, EditPoint, IndentConfig, OnEdit, SelectionState,
    SnapshotPreEdit, TextEditor,
};
pub use theme::EditTheme;
