//! # bevy_text_editor
//!
//! Editable text widget for Bevy: pointer interaction (scroll, click + drag
//! selection, copy) on top of [`bevy_text_engine`] `TextView` entities, plus
//! the editable-text core (cursor, selection, edit history, undo/redo,
//! clipboard, typed-character handling).
//!
//! ## Plugins
//!
//! - [`TextInteractionPlugin`] — read-only interaction: scroll, drag-select,
//!   copy. Pair with `TextEnginePlugins` for a selectable, scrollable view.
//! - [`TextEditorPlugin`] — typed-char input, edit history, undo/redo, paste,
//!   keyboard editing shortcuts. Pulls in `TextInteractionPlugin` automatically.
//!
//! ## Architecture
//!
//! - A custom `bevy_picking` backend in [`picking`] hit-tests the
//!   [`bevy_text_engine::TextViewViewport`] rect of every `TextView` and
//!   produces `PointerHits`.
//! - Observers in [`interaction`] consume `Pointer<Press|Drag|Release|Scroll>`
//!   events that picking has already routed to the right entity.
//! - Editing dispatch is observer-driven: `FocusedInput<KeyboardInput>` plus
//!   typed `*Requested` editing events that hosts (or this crate's plugin)
//!   write to.
//!
//! No polling systems, no manual cursor-rect hit tests.

pub mod anchor;
pub mod components;
pub mod cursor_movement;
pub mod edit;
pub mod editing_events;
pub mod handlers;
pub mod history;
pub mod interaction;
pub mod picking;
pub mod plugin;
pub mod prelude;
pub mod selection;
pub mod state;
pub mod typing;

pub use anchor::{Anchor, AnchorBias, AnchorRange, AnchorSet, TextEdit};
pub use edit::{point_at_byte, EditOutcome};
pub use components::{ScrollConfig, TextViewDragState};
pub use editing_events::*;
pub use history::{EditHistory, EditKind, EditOperation, EditTransaction};
pub use interaction::{copy_selection, screen_to_char_pos};
pub use plugin::{EditEmitSet, TextEditorPlugin, TextInteractionPlugin};
pub use selection::{Selection, SelectionCollection};
pub use state::{
    CursorState, EditDelta, EditHistoryState, EditPoint, IndentConfig, OnEdit, SelectionState,
    SnapshotPreEdit, TextEditor,
};
