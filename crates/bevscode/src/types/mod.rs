//! Core types for the code editor.
//!
//! Types for the editable-text core (selection, edit history, anchors,
//! cursor / selection / history Components, the `Selection` /
//! `SelectionCollection` types) live in [`bevy_instanced_text_edit`] and are
//! re-exported here so existing `crate::types::*` imports keep resolving.

pub mod display_map;
pub mod editor;
pub mod events;
pub mod fold;

// Re-export the editable-text core types from bevy_instanced_text_edit.
pub use bevy_instanced_text_edit::{
    Anchor, AnchorBias, AnchorSet, CursorState, EditHistory, EditHistoryState,
    EditKind, EditOperation, EditTransaction, IndentConfig, Selection, SelectionCollection,
    SelectionState, TextEdit, TextEditor,
};

pub use display_map::*;
pub use editor::*;
pub use fold::*;
