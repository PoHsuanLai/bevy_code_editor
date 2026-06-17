//! Core types for the code editor.

pub mod brackets;
pub mod events;
pub mod fold;
pub mod goto_line;
pub mod gutter;
pub mod marker;
pub mod overlays;
pub mod styling;

pub use bevy_instanced_text_editor::{
    Anchor, AnchorBias, AnchorSet, CursorState, EditHistory, EditHistoryState, EditKind,
    EditOperation, EditTransaction, IndentConfig, Selection, SelectionCollection, SelectionState,
    TextEdit, TextEditor,
};

/// Key-repeat state specialized to the editor's [`crate::input::EditorAction`].
pub type KeyRepeatState = bevy_instanced_text_editor::KeyRepeatState<crate::input::EditorAction>;

pub use brackets::*;
pub use events::*;
pub use fold::*;
pub use goto_line::*;
pub use gutter::*;
pub use marker::*;
pub use overlays::*;
pub use styling::*;
