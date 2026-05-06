//! Editable text widget for Bevy. [`TextInteractionPlugin`] adds scroll /
//! drag-select / copy on any `TextView`; [`TextEditorPlugin`] adds typed-char
//! input, edit history, undo/redo, clipboard, and keyboard shortcuts on top.
//!
//! Dispatch is observer-driven: a custom `bevy_picking` backend routes pointer
//! events to the right entity; editing events flow as typed `*Requested` messages.

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
