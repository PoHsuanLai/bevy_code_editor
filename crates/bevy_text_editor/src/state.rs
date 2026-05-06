//! Editable-text widget state Components.
//!
//! These attach to a [`bevy_text_engine::TextView`] entity to make it
//! editable. The [`TextEditor`] marker's `#[require]` cascade pulls in
//! everything needed for a working editable text field.

use bevy::prelude::*;
use ropey::Rope;

use crate::anchor::AnchorSet;
use crate::history::EditHistory;
use crate::selection::SelectionCollection;

use crate::components::{ScrollConfig, TextViewDragState};

/// Marker requesting that [`EditHistoryState`] keep a clone of the rope
/// from before each edit. Consumers that need pre-edit positions in the
/// LSP wire encoding (incremental `did_change`) attach this to the
/// editor entity. Plain text widgets don't need it — adding the marker
/// is opt-in so we don't pay the structural-clone cost when no one is
/// listening.
///
/// Ropey clones are O(log n) memory because of structural sharing, so
/// the cost is small per-edit. The snapshot is dropped same-frame, after
/// the [`OnEdit`] observer chain has read it.
#[derive(Component, Default, Clone, Copy, Debug, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct SnapshotPreEdit;

/// Primary cursor position and blink-tracking state.
///
/// `cursor_pos` mirrors `selections.primary().head_offset()`; handlers mutate it
/// during a keystroke and apply the full `SelectionCollection` update afterward.
/// `last_cursor_pos` drives auto-scroll detection; `last_cursor_pos_for_blink`
/// is tracked separately to avoid racing with the auto-scroll system.
#[derive(Component)]
pub struct CursorState {
    pub cursor_pos: usize,
    pub last_cursor_pos: usize,
    /// Seconds since app start when cursor last moved; resets blink animation.
    pub cursor_moved_time: f64,
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

#[derive(Component, Default)]
pub struct SelectionState {
    pub selections: SelectionCollection,
}

/// Undo/redo stacks, anchors, and transient pending-edit fields. Consumers
/// **observe [`OnEdit`]** rather than reading fields directly —
/// [`crate::plugin::emit_edit_triggers`] drains the fields each frame.
#[derive(Component)]
pub struct EditHistoryState {
    pub history: EditHistory,
    pub anchors: AnchorSet,
    /// Captured at edit-time; drained into [`OnEdit`] so consumers get byte
    /// offsets without needing the pre-edit rope.
    #[doc(hidden)]
    pub pending_byte_edit: Option<EditDelta>,
    /// Mirrored from the [`SnapshotPreEdit`] marker each frame so `replace_range`
    /// can decide whether to clone the rope without an extra Bevy query.
    #[doc(hidden)]
    pub snapshot_pre_edits: bool,
    /// Cloned before the edit when `snapshot_pre_edits` is set; used by LSP
    /// incremental sync. Drained into [`OnEdit`] by `emit_edit_triggers`.
    #[doc(hidden)]
    pub pre_edit_rope: Option<Rope>,
}

impl Default for EditHistoryState {
    fn default() -> Self {
        Self {
            history: EditHistory::default(),
            anchors: AnchorSet::new(),
            pending_byte_edit: None,
            snapshot_pre_edits: false,
            pre_edit_rope: None,
        }
    }
}

/// Emitted per edit by [`crate::plugin::emit_edit_triggers`]. Consumers add
/// observers to react. `byte_edit` is `None` for line-structure-only signals;
/// `pre_edit_rope` is `Some` only when [`SnapshotPreEdit`] is on the entity
/// (used by LSP incremental sync).
#[derive(Message, EntityEvent, Clone, Debug, Reflect)]
#[reflect(Clone, Debug)]
pub struct OnEdit {
    pub entity: Entity,
    pub byte_edit: Option<EditDelta>,
    #[reflect(ignore)]
    pub pre_edit_rope: Option<Rope>,
}

/// 0-indexed `(row, byte_column)`. Mirrors `tree_sitter::Point` without the dep.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub struct EditPoint {
    pub row: u32,
    pub column_byte: u32,
}

/// Edit snapshot captured at edit-time so consumers can build
/// `tree_sitter::InputEdit` or LSP `did_change` without the pre-edit rope.
#[derive(Clone, Copy, Debug, Reflect)]
pub struct EditDelta {
    pub start_byte: usize,
    pub old_end_byte: usize,
    pub new_end_byte: usize,
    pub start_position: EditPoint,
    pub old_end_position: EditPoint,
    pub new_end_position: EditPoint,
}

/// Indentation policy. Default: 4 spaces.
#[derive(Component, Clone, Copy, Debug, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct IndentConfig {
    pub tab_width: usize,
    /// `false` inserts a literal `\t`.
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

/// Marker for an editable text widget. `#[require]` cascades all supporting
/// state (and transitively `TextView` + engine components). Pair with
/// [`crate::TextEditorPlugin`].
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
