//! Editable-text widget state Components.
//!
//! These attach to a [`bevy_text_engine::TextView`] entity to make it
//! editable. The [`TextEditor`] marker's `#[require]` cascade pulls in
//! everything needed for a working editable text field.

use bevy::prelude::*;

use crate::anchor::AnchorSet;
use crate::history::EditHistory;
use crate::selection::SelectionCollection;

use crate::components::{ScrollConfig, TextViewDragState};

/// Cursor state component — tracks the primary cursor's position over time.
///
/// The set of active cursors and selection ranges lives in
/// [`SelectionState::selections`]. `cursor_pos` is a convenience mirror of
/// `selections.primary().head_offset()` that input handlers mutate during
/// a single keystroke; the matching update to the SelectionCollection is
/// applied at the end of the handler via
/// [`SelectionState::apply_primary_cursor`].
///
/// `last_cursor_pos` is consumed by editor-level auto-scroll systems to
/// detect movement between frames. The blink-tracker fields drive caret
/// fade-out without racing the auto-scroll system.
#[derive(Component)]
pub struct CursorState {
    /// Primary cursor position (char index). Mirror of `selections.primary().head_offset()`.
    pub cursor_pos: usize,

    /// Last cursor position (for detecting cursor movement)
    pub last_cursor_pos: usize,

    /// Time (in seconds since app start) when cursor was last moved
    /// Used to reset cursor blink animation after movement
    pub cursor_moved_time: f64,

    /// Last cursor position for blink reset tracking (separate from last_cursor_pos)
    /// This is tracked independently to avoid race conditions with auto_scroll_to_cursor
    pub last_cursor_pos_for_blink: usize,
}

impl Default for CursorState {
    fn default() -> Self {
        Self {
            cursor_pos: 0,
            last_cursor_pos: 0,
            cursor_moved_time: 0.0,
            last_cursor_pos_for_blink: 0,
        }
    }
}

/// Selection state component — owns the [`SelectionCollection`].
#[derive(Component, Default)]
pub struct SelectionState {
    /// Selection collection for managing multiple selections with edit-awareness
    pub selections: SelectionCollection,
}

/// Edit history and anchor state component.
///
/// Carries undo / redo stacks, the anchor set, and two transient
/// pending-edit fields edit ops record into. The
/// [`crate::plugin::emit_edit_triggers`] system reads those fields each
/// frame, fires an [`OnEdit`] event on the entity, and clears them.
///
/// Editor / consumer crates **observe `OnEdit`** rather than reading the
/// fields directly. The fields are an implementation detail of how
/// `EditHistoryState` records what changed within a single edit op; the
/// public contract for downstream consumers is the [`OnEdit`] event.
#[derive(Component)]
pub struct EditHistoryState {
    /// Edit history for undo/redo
    pub history: EditHistory,
    /// Anchor set for edit-resilient position tracking
    pub anchors: AnchorSet,
    /// Most recent edit, captured at edit-time so consumers can build
    /// `tree_sitter::InputEdit` (or LSP `did_change`) without needing the
    /// pre-edit rope. Drained into an [`OnEdit`] trigger by
    /// [`crate::plugin::emit_edit_triggers`].
    #[doc(hidden)]
    pub pending_byte_edit: Option<EditDelta>,
    /// Line index from which line-keyed entities should be invalidated.
    /// Set when an edit changes line structure; drained into [`OnEdit`].
    #[doc(hidden)]
    pub invalidate_lines_from: Option<usize>,
}

impl Default for EditHistoryState {
    fn default() -> Self {
        Self {
            history: EditHistory::default(),
            anchors: AnchorSet::new(),
            pending_byte_edit: None,
            invalidate_lines_from: None,
        }
    }
}

/// Emitted on a `TextEditor` entity after every edit operation.
///
/// Triggered by [`crate::plugin::emit_edit_triggers`] each Update; consumers
/// add observers (`app.add_observer(...)`) to react. Carries:
/// - `byte_edit`: `(start_byte, old_end_byte, new_end_byte)` for incremental
///   tree-sitter reparse, did_change LSP notifications, etc. Always `Some`
///   for real edits; may be `None` for trigger-only events that signal
///   only a line-structure invalidation.
/// - `invalidate_lines_from`: line index from which line-keyed entities
///   need re-spawning. `Some` when the edit changed line count (newline
///   insert/delete, multi-line paste, multi-line undo).
///
/// Bevy's per-entity event system: `commands.entity(e).trigger(OnEdit { … })`.
/// Observers receive `On<OnEdit>` and read `trigger.event_target()` for the
/// entity, plus the carried fields.
#[derive(Message, EntityEvent, Clone, Copy, Debug, Reflect)]
#[reflect(Clone, Debug)]
pub struct OnEdit {
    /// The editor entity whose buffer was edited.
    pub entity: Entity,
    /// Edit delta with byte offsets and pre/post positions. `None` if this
    /// trigger only signals a line-structure invalidation.
    pub byte_edit: Option<EditDelta>,
    /// Line index from which to invalidate line-keyed entities. `None`
    /// when no lines were added or removed.
    pub invalidate_lines_from: Option<usize>,
}

/// A 0-indexed `(row, byte_column)` position into the rope. Mirrors
/// `tree_sitter::Point` without depending on tree-sitter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub struct EditPoint {
    pub row: u32,
    pub column_byte: u32,
}

/// Snapshot of one edit, captured when the edit happens so consumers can
/// build `tree_sitter::InputEdit` (or LSP `did_change`) without needing
/// the pre-edit rope.
#[derive(Clone, Copy, Debug, Reflect)]
pub struct EditDelta {
    pub start_byte: usize,
    pub old_end_byte: usize,
    pub new_end_byte: usize,
    pub start_position: EditPoint,
    pub old_end_position: EditPoint,
    pub new_end_position: EditPoint,
}

/// Per-editor indentation policy for tab insertion.
///
/// The default — 4 spaces — matches what most editors do. Hosts that want
/// hard tabs set `use_spaces = false`; soft-tab width scales `tab_width`.
#[derive(Component, Clone, Copy, Debug, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct IndentConfig {
    /// Number of spaces per tab when `use_spaces` is `true`.
    pub tab_width: usize,
    /// `true` to insert spaces, `false` to insert one '\t' per tab keypress.
    pub use_spaces: bool,
}

impl Default for IndentConfig {
    fn default() -> Self {
        Self {
            tab_width: 4,
            use_spaces: true,
        }
    }
}

/// Marker component for an editable text widget.
///
/// `#[require]` cascades the supporting state Components, so spawning a
/// `TextEditor` is sufficient to get a fully functional editable text view
/// (a la `bevy_text::Text2d`). The cascaded `TextView` transitively brings
/// the engine rendering components.
///
/// Pair with [`crate::TextEditorPlugin`] to register the editing systems.
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
#[require(
    bevy_text_engine::TextView,
    CursorState,
    SelectionState,
    EditHistoryState,
    IndentConfig,
    TextViewDragState,
    ScrollConfig,
)]
pub struct TextEditor;
