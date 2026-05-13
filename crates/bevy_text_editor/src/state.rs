//! Editor-only state components: edit history, indentation, the TextEditor
//! marker, and the per-edit byte snapshot.
//!
//! The cursor + selection components live in [`bevy_text_interaction`] —
//! editors and terminals share them. Everything in this module is
//! rope-specific or only meaningful when undo / LSP-style edit tracking
//! is in play.

use bevy::prelude::*;
use ropey::Rope;

use bevy_text_interaction::anchor::AnchorSet;
use crate::history::EditHistory;

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
/// state. Pair with [`crate::InstancedTextEditPlugin`].
///
/// **Includes [`bevy_instanced_text::TextBuffer<crate::RopeBuffer>`]** in
/// the cascade, which in turn (via the renderer's
/// `register_required_components` for `TextBuffer<T>`) brings in every
/// renderer component a text view needs: `DisplayLayout`, `TextFont`,
/// `MonoFontFaces`, `MonoCellWidth`, `bevy::ui::ScrollPosition`,
/// `SmoothScroll`, `ContentMetrics`, `LineStyles`, `HiddenLines`,
/// `LayoutTuning`, `Node`, `Transform`, `Visibility`, `Pickable`.
/// Spawning just `TextEditor` is enough to get a fully-rendered editor.
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
#[require(
    bevy_instanced_text::TextBuffer<crate::rope_content::RopeBuffer>,
    bevy_text_interaction::CursorState,
    bevy_text_interaction::SelectionState,
    EditHistoryState,
    IndentConfig,
    bevy_text_interaction::TextViewDragState,
    bevy_text_interaction::ScrollConfig,
    bevy_text_interaction::CursorSettings,
    bevy_text_interaction::BlinkPhase,
    bevy_text_interaction::InteractionSettings,
)]
pub struct TextEditor;
