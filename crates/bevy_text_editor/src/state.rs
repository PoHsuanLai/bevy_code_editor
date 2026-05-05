//! Editable-text widget state Components.
//!
//! These attach to a [`bevy_text_engine::TextView`] entity to make it
//! editable. The [`TextEditor`] marker's `#[require]` cascade pulls in
//! everything needed for a working editable text field.

use bevy::prelude::*;

use crate::anchor::AnchorSet;
use crate::history::EditHistory;
use crate::selection::SelectionCollection;

use crate::components::{ScrollConfig, TextViewDragState, TextViewSelectionState};

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
/// Carries undo / redo stacks, the anchor set, and two side-channel fields
/// edit ops record into so editor-level systems (incremental tree-sitter
/// reparse, line-entity invalidation) can pick up what changed without
/// re-diffing the rope:
///
/// - `pending_byte_edit` — the `(start_byte, old_end_byte, new_end_byte)` of
///   the most recent edit. Tree-sitter incremental parsing wants this; other
///   consumers can ignore it. Cleared by editor systems after read.
/// - `invalidate_lines_from` — when an edit changes line count, this records
///   the line index from which subsequent line entities should be re-spawned.
///   Cleared by editor systems after read.
///
/// Both are `Option`s and harmless to ignore — read-only / non-IDE hosts
/// don't pay for them.
#[derive(Component)]
pub struct EditHistoryState {
    /// Edit history for undo/redo
    pub history: EditHistory,
    /// Anchor set for edit-resilient position tracking
    pub anchors: AnchorSet,
    /// Most recent edit, as a byte range. Set by every edit op; cleared by
    /// downstream consumers (e.g. tree-sitter reparse) after read.
    pub pending_byte_edit: Option<(usize, usize, usize)>,
    /// Line index from which line-keyed entities should be invalidated.
    /// Set when an edit changes line structure (newline insert/delete,
    /// multi-line paste, multi-line undo). Cleared after read.
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
    TextViewSelectionState,
    ScrollConfig,
)]
pub struct TextEditor;
