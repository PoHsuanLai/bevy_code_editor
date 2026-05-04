//! Editor component types — pure data definitions, no logic.
//!
//! Operational methods live in `input/`:
//! - `input/editor_ops.rs` — impl CodeEditorState (search, cursor movement)
//! - `input/selection_ops.rs` — impl SelectionState (multi-cursor, selection sync)
//! - `input/editing.rs` — impl EditHistoryState (insert, delete, undo/redo, anchors)

use bevy::prelude::*;
use std::time::Instant;

use super::anchor::AnchorSet;
use super::display_map::{DisplayMap, HighlightedToken, LineSegment};
use super::history::EditHistory;
use super::selection::{Cursor, SelectionCollection};

/// Configuration for viewport behavior
#[derive(Resource, Clone, Copy, Debug)]
pub struct ViewportConfig {
    /// If true, viewport automatically resizes to match window size.
    /// If false, you must manually set ViewportDimensions.
    /// Default: true (for backward compatibility)
    pub auto_resize_to_window: bool,
}

impl Default for ViewportConfig {
    fn default() -> Self {
        Self {
            auto_resize_to_window: true,
        }
    }
}

/// Viewport dimensions and layout information
///
/// This resource tracks both the viewport size and the computed layout for rendering.
/// The UI plugin (or custom UI) is responsible for computing the layout based on
/// its own settings and updating this resource.
#[derive(Resource, Clone, Copy, Debug)]
pub struct ViewportDimensions {
    /// Viewport width in pixels
    pub width: u32,

    /// Viewport height in pixels
    pub height: u32,

    /// Top-left position of the editor panel in window/screen pixels (set by host app)
    pub screen_position: Vec2,

    // === Computed Layout (set by UI plugin) ===
    /// Left margin/padding before text starts
    pub text_area_left: f32,

    /// Top margin/padding before text starts
    pub text_area_top: f32,

    /// Width of the gutter area (line numbers, etc.)
    pub gutter_width: f32,

    /// X position of the separator line between gutter and code
    pub separator_x: f32,
}

impl ViewportDimensions {
    /// Calculate the world coordinate of the viewport's left edge
    pub fn world_left(&self) -> f32 {
        if self.screen_position == Vec2::ZERO {
            // Auto-resize mode (default): viewport is centered at (0,0)
            -(self.width as f32) / 2.0
        } else {
            // Manual mode: screen_position.x is the left edge
            self.screen_position.x
        }
    }

    /// Calculate the world coordinate of the viewport's top edge
    pub fn world_top(&self) -> f32 {
        if self.screen_position == Vec2::ZERO {
            // Auto-resize mode (default): viewport is centered at (0,0)
            self.height as f32 / 2.0
        } else {
            // Manual mode: screen_position.y is the top edge
            self.screen_position.y
        }
    }
}

impl Default for ViewportDimensions {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            screen_position: Vec2::ZERO,
            // Default layout values (can be overridden by UI plugin)
            text_area_left: 80.0,
            text_area_top: 10.0,
            gutter_width: 60.0,
            separator_x: 70.0,
        }
    }
}

/// Marker component for the code editor entity.
///
/// The editor entity has `CodeEditor` + `CodeEditorState` + `CursorState` + `TextViewState` + `TextViewViewport`.
#[derive(Component, Default)]
pub struct CodeEditor;

/// Cursor state component — tracks cursor positions and multi-cursor state.
#[derive(Component)]
pub struct CursorState {
    /// Cursor position (char index) - primary cursor for backward compatibility
    pub cursor_pos: usize,

    /// Last cursor position (for detecting cursor movement)
    pub last_cursor_pos: usize,

    /// Time (in seconds since app start) when cursor was last moved
    /// Used to reset cursor blink animation after movement
    pub cursor_moved_time: f64,

    /// Last cursor position for blink reset tracking (separate from last_cursor_pos)
    /// This is tracked independently to avoid race conditions with auto_scroll_to_cursor
    pub last_cursor_pos_for_blink: usize,

    /// All cursors (including primary cursor at index 0)
    /// The first cursor is the "primary" cursor that maps to cursor_pos/selection_start/selection_end
    pub cursors: Vec<Cursor>,
}

impl Default for CursorState {
    fn default() -> Self {
        Self {
            cursor_pos: 0,
            last_cursor_pos: 0,
            cursor_moved_time: 0.0,
            last_cursor_pos_for_blink: 0,
            cursors: vec![Cursor::new(0)],
        }
    }
}

/// Main editor state — focus tracking and marker.
///
/// Most fields have been extracted into focused components:
/// - `SelectionState` — selection_start, selection_end, selections
/// - `EditHistoryState` — history, anchors
/// - `SyntaxCacheState` — tokens, lines, last_highlighted_version, last_lines_version, tree-sitter state
/// - `EditorDisplayState` — display_map, entity_pool, line_number_pool, invalidate_lines_from
/// - `CursorState` — cursor_pos, cursors, etc.
/// - `TextViewState` — rope, scroll, rendering state
#[derive(Component)]
pub struct CodeEditorState {
    /// Is editor focused
    pub is_focused: bool,
}

impl Default for CodeEditorState {
    fn default() -> Self {
        Self { is_focused: false }
    }
}

/// Selection state component — tracks selection positions and the SelectionCollection.
#[derive(Component)]
pub struct SelectionState {
    /// Selection start (None = no selection) - primary cursor for backward compatibility
    pub selection_start: Option<usize>,
    /// Selection end - primary cursor for backward compatibility
    pub selection_end: Option<usize>,
    /// Selection collection for managing multiple selections with edit-awareness
    pub selections: SelectionCollection,
}

impl Default for SelectionState {
    fn default() -> Self {
        Self {
            selection_start: None,
            selection_end: None,
            selections: SelectionCollection::new(),
        }
    }
}

/// Edit history and anchor state component.
#[derive(Component)]
pub struct EditHistoryState {
    /// Edit history for undo/redo
    pub history: EditHistory,
    /// Anchor set for edit-resilient position tracking
    pub anchors: AnchorSet,
}

impl Default for EditHistoryState {
    fn default() -> Self {
        Self {
            history: EditHistory::default(),
            anchors: AnchorSet::new(),
        }
    }
}

/// Syntax highlighting cache state component.
#[derive(Component)]
pub struct SyntaxCacheState {
    /// Cached highlighted tokens
    pub tokens: Vec<HighlightedToken>,
    /// Cached processed lines for rendering (optimization)
    pub lines: Vec<Vec<LineSegment>>,
    /// Last content version when highlighting was run
    pub last_highlighted_version: u64,
    /// Last content version when line segments were built (PERFORMANCE)
    pub last_lines_version: u64,
    /// Last syntax tree version that was rendered (PERFORMANCE)
    #[cfg(feature = "tree-sitter")]
    pub last_rendered_tree_version: u64,
    /// Pending text edit for tree-sitter incremental parsing
    /// Format: (start_byte, old_end_byte, new_end_byte)
    #[cfg(feature = "tree-sitter")]
    pub pending_tree_sitter_edit: Option<(usize, usize, usize)>,
}

impl Default for SyntaxCacheState {
    fn default() -> Self {
        Self {
            tokens: Vec::new(),
            lines: Vec::new(),
            last_highlighted_version: u64::MAX, // Force initial highlighting
            last_lines_version: 0,
            #[cfg(feature = "tree-sitter")]
            last_rendered_tree_version: 0,
            #[cfg(feature = "tree-sitter")]
            pending_tree_sitter_edit: None,
        }
    }
}

/// Editor display state component — entity pools, display map, line invalidation.
#[derive(Component)]
pub struct EditorDisplayState {
    /// Display map for soft line wrapping
    pub display_map: DisplayMap,
    /// Pool of reusable text entities (PERFORMANCE)
    pub entity_pool: Vec<Entity>,
    /// Pool of reusable line number entities (PERFORMANCE)
    pub line_number_pool: Vec<Entity>,
    /// When line count changes, stores the line index from which all subsequent
    /// line entities should be invalidated.
    pub invalidate_lines_from: Option<usize>,
}

impl Default for EditorDisplayState {
    fn default() -> Self {
        Self {
            display_map: DisplayMap::default(),
            entity_pool: Vec::new(),
            line_number_pool: Vec::new(),
            invalidate_lines_from: None,
        }
    }
}

/// Component markers for editor entities

#[derive(Component)]
pub struct EditorText;

#[derive(Component)]
pub struct HighlightedTextToken {
    pub index: usize,
}

#[derive(Component)]
pub struct EditorCursor {
    /// Index of this cursor in the cursors array (0 = primary cursor)
    pub cursor_index: usize,
}

#[derive(Component)]
pub struct LineNumbers;

#[derive(Component)]
pub struct Separator;

#[derive(Component)]
pub struct SelectionHighlight {
    pub line_index: usize,
    /// Index of the cursor this selection belongs to (0 = primary cursor)
    pub cursor_index: usize,
}

/// Component marker for bracket match highlight entities (bounding box style)
#[derive(Component)]
pub struct BracketMatchHighlight {
    /// Which bracket this belongs to (0 = cursor bracket, 1 = matching bracket)
    pub bracket_index: usize,
    /// Which border edge (0=top, 1=bottom, 2=left, 3=right)
    pub edge: usize,
}

/// Component marker for current line border (top or bottom line)
#[derive(Component)]
pub struct CursorLineBorder {
    /// The cursor index this border belongs to (for multi-cursor support)
    pub cursor_index: usize,
    /// Whether this is the top (true) or bottom (false) border
    pub is_top: bool,
}

/// Component marker for current word highlight (under cursor)
#[derive(Component)]
pub struct CursorWordHighlight {
    /// The cursor index this highlight belongs to (for multi-cursor support)
    pub cursor_index: usize,
}

/// Component marker for indent guide entities
#[derive(Component)]
pub struct IndentGuide {
    /// The indentation level (0 = first indent, 1 = second indent, etc.)
    pub level: usize,
    /// The line index this guide is on
    pub line_index: usize,
}

/// Component marker for the minimap background
#[derive(Component)]
pub struct MinimapBackground;

/// Component marker for the minimap viewport slider (appears on hover)
#[derive(Component)]
pub struct MinimapSlider;

/// Component marker for the minimap viewport highlight (subtle, always visible)
#[derive(Component)]
pub struct MinimapViewportHighlight;

/// Component marker for the minimap scrollbar
#[derive(Component)]
pub struct MinimapScrollbar;

/// Component marker for minimap line entities
#[derive(Component)]
pub struct MinimapLine {
    /// The line index this represents
    pub line_index: usize,
}

/// Component marker for minimap search match highlights
#[derive(Component)]
pub struct MinimapFindHighlight {
    /// The line index this highlight represents
    pub line_index: usize,
}

/// Component marker for GPU minimap mesh entity
#[derive(Component)]
pub struct GpuMinimapMesh {
    /// The content version when this mesh was built
    pub built_at_version: u64,
    /// The scroll offset when this mesh was built
    pub built_at_scroll: f32,
    /// The viewport width when this mesh was built
    pub built_at_width: u32,
    /// The viewport height when this mesh was built
    pub built_at_height: u32,
    /// The viewport offset_x when this mesh was built
    pub built_at_offset_x: f32,
}

/// Component marker for the minimap camera
#[derive(Component)]
pub struct MinimapCamera;

/// Resource to track minimap hover state
#[derive(Resource, Default)]
pub struct MinimapHoverState {
    /// Whether the mouse is currently hovering over the minimap
    pub is_hovered: bool,
}

/// Resource to track minimap drag state for click-to-scroll and drag-to-scroll
#[derive(Resource, Default)]
pub struct MinimapDragState {
    /// Whether we're currently dragging the minimap slider
    pub is_dragging: bool,
    /// Whether we're dragging the viewport highlight (vs clicking elsewhere on minimap)
    pub is_dragging_highlight: bool,
    /// Initial mouse Y position when drag started (for highlight dragging)
    pub drag_start_y: f32,
    /// Initial scroll offset when drag started (for highlight dragging)
    pub drag_start_scroll: f32,
}

/// Resource to track key repeat state for editor actions
#[derive(Resource, Default)]
pub struct KeyRepeatState {
    /// The action currently being repeated (if any)
    pub current_action: Option<crate::input::EditorAction>,
    /// When the action key was first pressed
    pub press_start: Option<Instant>,
    /// When the last repeat occurred
    pub last_repeat: Option<Instant>,
}

/// Represents a matched bracket pair
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BracketMatch {
    /// Position of the bracket at/near cursor
    pub cursor_bracket_pos: usize,
    /// Position of the matching bracket
    pub matching_bracket_pos: usize,
}

/// Resource to track the current bracket match state
#[derive(Resource, Default, Clone, Debug)]
pub struct BracketMatchState {
    /// Current bracket match (if any)
    pub current_match: Option<BracketMatch>,
}

/// Component marker for find/search highlight entities
#[derive(Component)]
pub struct FindHighlight {
    /// Index of this match in the matches list
    pub match_index: usize,
}

/// Event emitted when save is requested (Ctrl+S)
/// The host application should handle this event to save the buffer contents.
#[derive(bevy::prelude::Message, Clone, Debug)]
pub struct SaveRequested {
    /// The current buffer content
    pub content: String,
}

/// Event emitted when open is requested (Ctrl+O)
/// The host application should handle this event to show a file picker.
#[derive(bevy::prelude::Message, Clone, Debug)]
pub struct OpenRequested;

// ========== External Scroll Control ==========

/// Resource for external UI (egui) to control and read editor scroll state
///
/// This provides bidirectional communication:
/// - External UI reads current scroll position and content dimensions
/// - External UI sends scroll requests via pending_* fields
/// - bevy_code_editor's systems apply these requests and update current state
#[derive(Resource, Default)]
pub struct EditorScrollControl {
    /// Current vertical scroll offset (negative when scrolled down)
    /// Updated by bevy_code_editor every frame
    pub scroll_offset: f32,

    /// Current horizontal scroll offset
    /// Updated by bevy_code_editor every frame
    pub horizontal_scroll_offset: f32,

    /// Total content height in pixels
    pub content_height: f32,

    /// Total content width in pixels (longest line)
    pub content_width: f32,

    /// Visible viewport height in pixels
    pub viewport_height: f32,

    /// Visible viewport width in pixels (excluding gutter)
    pub viewport_width: f32,

    /// Line height for line-based scrolling
    pub line_height: f32,

    /// Pending vertical scroll request (absolute offset to scroll to)
    /// Set by external UI, cleared by bevy_code_editor when applied
    pub pending_scroll_to: Option<f32>,

    /// Pending vertical scroll delta (relative scroll amount)
    /// Set by external UI, cleared by bevy_code_editor when applied
    pub pending_scroll_delta: Option<f32>,

    /// Pending horizontal scroll request (absolute offset)
    pub pending_horizontal_scroll_to: Option<f32>,

    /// Pending horizontal scroll delta (relative amount)
    pub pending_horizontal_scroll_delta: Option<f32>,
}
