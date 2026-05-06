//! Typed events emitted by `dispatch_action_events` — one per
//! [`super::EditorAction`] variant.
//!
//! The 33 editing events (cursor movement, selection, delete / insert /
//! clipboard / undo / redo) are defined in [`bevy_text_editor`] and
//! registered by `TextEditorPlugin`; they're re-exported here so dispatcher
//! code stays at `crate::input::action_events::*`.
//!
//! The IDE-only events (replace, goto-line, multi-cursor, folding, LSP
//! request, save / open) live in this module.
//!
//! All events are unit-style structs today.

use bevy::prelude::*;

// Re-export the editing events from bevy_text_editor (moved as part of the
// editable-text-widget refactor).
pub use bevy_text_editor::{
    ClearSelectionRequested, CopyRequested, CutRequested, DeleteBackwardRequested,
    DeleteForwardRequested, DeleteLineRequested, DeleteWordBackwardRequested,
    DeleteWordForwardRequested, InsertNewlineRequested, InsertTabRequested,
    MoveCursorDocumentEndRequested, MoveCursorDocumentStartRequested, MoveCursorDownRequested,
    MoveCursorLeftRequested, MoveCursorLineEndRequested, MoveCursorLineStartRequested,
    MoveCursorPageDownRequested, MoveCursorPageUpRequested, MoveCursorRightRequested,
    MoveCursorUpRequested, MoveCursorWordLeftRequested, MoveCursorWordRightRequested,
    PasteRequested, RedoRequested, SelectAllRequested, SelectDownRequested,
    SelectLeftRequested, SelectLineEndRequested, SelectLineStartRequested,
    SelectRightRequested, SelectUpRequested, SelectWordLeftRequested,
    SelectWordRightRequested, UndoRequested,
};

macro_rules! action_event {
    ($($name:ident),* $(,)?) => {
        $(
            #[derive(Message, Clone, Copy, Debug, Default, Reflect)]
            #[reflect(Clone, Debug, Default)]
            pub struct $name;
        )*
    };
}

// Navigation
action_event!(GotoLineRequested);

// LSP
action_event!(
    RequestCompletionRequested,
    GotoDefinitionRequested,
    RenameSymbolRequested,
);

// Multi-cursor
action_event!(
    AddCursorAtNextOccurrenceRequested,
    AddCursorAboveRequested,
    AddCursorBelowRequested,
    ClearSecondaryCursorsRequested,
);

// Code folding
action_event!(
    ToggleFoldRequested,
    FoldRequested,
    UnfoldRequested,
    FoldAllRequested,
    UnfoldAllRequested,
);

// File operations are NOT defined here — they reuse `SaveRequested` /
// `OpenRequested` from `crate::types::events` which existed before this
// refactor and are part of the editor's public host-facing API.
