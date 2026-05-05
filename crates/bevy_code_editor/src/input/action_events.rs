//! Typed events emitted by `dispatch_action_events` — one per
//! [`super::EditorAction`] variant.
//!
//! Hosts and plugins consume these events instead of the giant action match
//! that used to live in `actions.rs::execute_action_core`. Every variant of
//! `EditorAction` corresponds to exactly one event here, with two exceptions
//! that reuse pre-existing public events:
//!   - `EditorAction::Save`  → [`crate::types::SaveRequested`]
//!   - `EditorAction::Open`  → [`crate::types::OpenRequested`]
//!
//! The keymap currency stays an `EditorAction` enum (leafwing-input-manager
//! requires an `Actionlike`); these events are an additional layer that the
//! dispatch system fans out into.
//!
//! All events are unit-style structs today. Future events that need
//! parameters (e.g. `MoveCursorTo { position: usize }`) can grow fields
//! without breaking the dispatch macro: just write the field-bearing
//! variant by hand and skip the macro for it.

use bevy::prelude::*;

/// Compact event definition. Each invocation generates a `Message`-deriving,
/// `Default`-deriving unit struct ready to be registered with `add_message`.
macro_rules! action_event {
    ($($name:ident),* $(,)?) => {
        $(
            #[derive(Message, Clone, Copy, Debug, Default, Reflect)]
            #[reflect(Clone, Debug, Default)]
            pub struct $name;
        )*
    };
}

// Deletion
action_event!(
    DeleteBackwardRequested,
    DeleteForwardRequested,
    DeleteWordBackwardRequested,
    DeleteWordForwardRequested,
    DeleteLineRequested,
);

// Special insertion
action_event!(InsertNewlineRequested, InsertTabRequested);

// Cursor movement
action_event!(
    MoveCursorLeftRequested,
    MoveCursorRightRequested,
    MoveCursorUpRequested,
    MoveCursorDownRequested,
    MoveCursorWordLeftRequested,
    MoveCursorWordRightRequested,
    MoveCursorLineStartRequested,
    MoveCursorLineEndRequested,
    MoveCursorDocumentStartRequested,
    MoveCursorDocumentEndRequested,
    MoveCursorPageUpRequested,
    MoveCursorPageDownRequested,
);

// Selection
action_event!(
    SelectLeftRequested,
    SelectRightRequested,
    SelectUpRequested,
    SelectDownRequested,
    SelectWordLeftRequested,
    SelectWordRightRequested,
    SelectLineStartRequested,
    SelectLineEndRequested,
    SelectAllRequested,
    ClearSelectionRequested,
);

// Clipboard
action_event!(CopyRequested, CutRequested, PasteRequested);

// Undo/Redo
action_event!(UndoRequested, RedoRequested);

// Search
action_event!(ReplaceRequested);

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
