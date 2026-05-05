//! Typed editing-request events — the public contract between hosts (or the
//! code editor's leafwing dispatcher) and this crate's editing handlers.
//!
//! All events are unit-style structs today. The crate's [`crate::TextEditorPlugin`]
//! registers them; per-action handler systems consume them.

use bevy::prelude::*;

macro_rules! editing_event {
    ($($name:ident),* $(,)?) => {
        $(
            #[derive(Message, Clone, Copy, Debug, Default, Reflect)]
            #[reflect(Clone, Debug, Default)]
            pub struct $name;
        )*
    };
}

// Cursor movement (12)
editing_event!(
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

// Selection (10)
editing_event!(
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

// Editing (7)
editing_event!(
    DeleteBackwardRequested,
    DeleteForwardRequested,
    DeleteWordBackwardRequested,
    DeleteWordForwardRequested,
    DeleteLineRequested,
    InsertNewlineRequested,
    InsertTabRequested,
);

// Clipboard (3)
editing_event!(CopyRequested, CutRequested, PasteRequested);

// Undo / redo (2)
editing_event!(UndoRequested, RedoRequested);
