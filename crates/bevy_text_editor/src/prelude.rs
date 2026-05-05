//! Convenient re-exports for hosts wiring up `TextView` interactivity and
//! editable text.
//!
//! Pair with `bevy_text_engine::prelude::*` which supplies the rendering
//! primitives (`TextView`, `TextViewState`, `TextViewViewport`,
//! `FontConfig`, the engine plugin group).

pub use crate::components::{ScrollConfig, TextViewDragState};
pub use crate::interaction::{copy_selection, screen_to_char_pos};
pub use crate::plugin::{TextEditorPlugin, TextInteractionPlugin};
pub use crate::selection::{Selection, SelectionCollection};
pub use crate::state::{
    CursorState, EditHistoryState, IndentConfig, OnEdit, SelectionState, TextEditor,
};
