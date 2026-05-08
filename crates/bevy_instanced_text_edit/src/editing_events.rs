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

/// Replace `[start_char..end_char]` with `text` on a specific editor entity.
/// The single payloaded edit primitive for ECS-driven mutation: LSP, completion,
/// formatting, AI suggestions, refactoring tools etc. emit this and let
/// `bevy_instanced_text_edit` route it through `EditHistoryState::replace_range` so
/// history, anchors, content_version, and `OnEdit` all stay correct.
#[derive(Message, Clone, Debug, Reflect)]
#[reflect(Clone, Debug)]
pub struct ReplaceRangeRequested {
    pub entity: Entity,
    pub start_char: usize,
    pub end_char: usize,
    pub text: String,
    pub kind: crate::history::EditKind,
    pub record_history: bool,
}

/// Replace the entire buffer of an editor entity with `text`. Skips history.
#[derive(Message, Clone, Debug, Reflect)]
#[reflect(Clone, Debug)]
pub struct SetTextRequested {
    pub entity: Entity,
    pub text: String,
}

/// Outbound: the primary cursor moved this frame. Hosts subscribe to mirror
/// the caret in a status bar, minimap, breadcrumb trail, or external view.
/// Emitted at most once per editor per frame; the emitter dedupes against
/// the previous frame's `cursor_pos`. `from`/`to` are rope char offsets.
#[derive(Message, Clone, Copy, Debug, Reflect)]
#[reflect(Clone, Debug)]
pub struct CursorMoved {
    pub entity: Entity,
    pub from: usize,
    pub to: usize,
}

/// Outbound: the selection set changed this frame (added/removed selections,
/// any anchor or head moved, mode flipped). Emitted at most once per editor
/// per frame, deduped against the previous frame's selection state. Hosts
/// rebuild selection-driven UI (find-in-selection counts, highlight overlays
/// in other panes) on this signal.
#[derive(Message, Clone, Debug, Reflect)]
#[reflect(Clone, Debug)]
pub struct SelectionChanged {
    pub entity: Entity,
    /// Number of selections after the change. `1` means a single primary
    /// caret/range; `>1` means multi-cursor mode.
    pub selection_count: usize,
    /// Total number of characters covered by all selections (`0` when every
    /// selection is empty — i.e. just carets).
    pub total_chars_selected: usize,
}
