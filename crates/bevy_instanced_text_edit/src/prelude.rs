//! Convenient re-exports for hosts wiring up `TextView` interactivity and
//! editable text.
//!
//! Pair with `bevy_instanced_text::prelude::*` which supplies the rendering
//! primitives (`TextView`, `TextBuffer`, `ScrollState`, `ContentMetrics`,
//! `TextFont`, the engine plugin group).

pub use crate::components::{ScrollConfig, TextViewDragState};
pub use crate::cursor_settings::{BlinkPhase, CursorSettings, CursorStyle};
pub use crate::editing_events::{ReplaceRangeRequested, SetTextRequested};
pub use crate::history::EditKind;
pub use crate::interaction::{copy_selection, screen_to_char_pos};
pub use crate::plugin::{InstancedTextEditPlugin, InstancedTextInteractionPlugin};
pub use crate::selection::{Selection, SelectionCollection};
pub use crate::state::{
    CursorState, EditHistoryState, IndentConfig, OnEdit, SelectionState, TextEditor,
};
pub use crate::theme::{TextCursorColor, TextSelectionColor};
