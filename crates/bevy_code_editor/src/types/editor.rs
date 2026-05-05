//! Editor component types — pure data definitions, no logic.
//!
//! The editable-text Components (`CursorState`, `SelectionState`,
//! `EditHistoryState`) are defined in [`bevy_text_editor`] and re-exported
//! through `crate::types`. Operational helpers live in `input/`:
//! - `input/editor_ops.rs` — free fns for search and editor cursor movement
//! - `input/multi_cursor.rs` — multi-cursor add/remove

use bevy::prelude::*;
use std::time::Instant;

use super::display_map::{HighlightedToken, LineSegment};

/// Configuration for viewport behavior
#[derive(Resource, Clone, Copy, Debug, Reflect)]
#[reflect(Resource, Default, Debug)]
pub struct ViewportConfig {
    /// If true, viewport automatically resizes to match window size.
    /// If false, you must manually set [`ViewportDimensions`].
    /// Default: true.
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
#[derive(Resource, Clone, Copy, Debug, Reflect)]
#[reflect(Resource, Default, Debug)]
pub struct ViewportDimensions {
    /// Viewport width in pixels
    pub width: u32,

    /// Viewport height in pixels
    pub height: u32,

    /// How the viewport's top-left maps to world coordinates.
    /// See [`crate::text_view::ViewportOrigin`].
    pub origin: crate::text_view::ViewportOrigin,

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
    /// World-space top-left of the viewport, resolving the origin enum.
    pub fn origin_position(&self) -> Vec2 {
        match self.origin {
            crate::text_view::ViewportOrigin::CenteredOrtho => Vec2::new(
                -(self.width as f32) / 2.0,
                self.height as f32 / 2.0,
            ),
            crate::text_view::ViewportOrigin::ScreenAbsolute(p) => p,
        }
    }

    /// Calculate the world coordinate of the viewport's left edge
    pub fn world_left(&self) -> f32 {
        self.origin_position().x
    }

    /// Calculate the world coordinate of the viewport's top edge
    pub fn world_top(&self) -> f32 {
        self.origin_position().y
    }
}

impl Default for ViewportDimensions {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            origin: crate::text_view::ViewportOrigin::CenteredOrtho,
            // Default layout values (can be overridden by UI plugin)
            text_area_left: 80.0,
            text_area_top: 10.0,
            gutter_width: 60.0,
            separator_x: 70.0,
        }
    }
}

/// Marker component for a code editor entity.
///
/// `#[require]` cascades [`bevy_text_editor::TextEditor`] (which transitively
/// brings the engine `TextView`, cursor / selection / edit-history state,
/// and pointer-interaction state) plus the IDE-specific Components — fold
/// state, bracket matching, syntax cache, scrollbar drag, goto-line dialog,
/// LSP UI state. Spawning a `CodeEditor` is sufficient for a fully
/// functional editor entity.
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
#[cfg_attr(
    not(feature = "lsp"),
    require(
        bevy_text_editor::TextEditor,
        SyntaxCacheState,
        EditorDisplayState,
        BracketMatchState,
        crate::types::fold::GotoLineState,
        crate::types::fold::FoldState,
        crate::plugin::scrollbar::ScrollbarDragState,
    )
)]
#[cfg_attr(
    feature = "lsp",
    require(
        bevy_text_editor::TextEditor,
        SyntaxCacheState,
        EditorDisplayState,
        BracketMatchState,
        crate::types::fold::GotoLineState,
        crate::types::fold::FoldState,
        crate::plugin::scrollbar::ScrollbarDragState,
        // LSP-side state. `LspDocument` is NOT in this cascade because it
        // requires a URI which the host must supply.
        bevy_lsp::LspClient,
        bevy_lsp::ServerCapabilities,
        crate::lsp_ui::state::LspCompletionPopup,
        crate::lsp_ui::state::LspHoverPopup,
        crate::lsp_ui::state::LspSignatureHelpPopup,
        crate::lsp_ui::state::LspCodeActionsPopup,
        crate::lsp_ui::state::LspInlayHints,
        crate::lsp_ui::state::LspDocumentHighlights,
        crate::lsp_ui::state::LspRenamePopup,
        crate::lsp_ui::state::LspDebounceTimers,
        crate::lsp_ui::state::LspSyncStateExtra,
    )
)]
pub struct CodeEditor;

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
    /// Pending text edit for tree-sitter incremental parsing.
    #[cfg(feature = "tree-sitter")]
    pub pending_tree_sitter_edit: Option<bevy_text_editor::EditDelta>,
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

/// Editor display state component — entity pools and line invalidation.
///
/// Soft-wrap and fold information now lives on the entity's `DisplayLayout`
/// produced by `display_map::build_display_layout`; this component only
/// holds entity-pool bookkeeping.
#[derive(Component, Default)]
pub struct EditorDisplayState {
    /// Pool of reusable text entities (PERFORMANCE)
    pub entity_pool: Vec<Entity>,
    /// Pool of reusable line number entities (PERFORMANCE)
    pub line_number_pool: Vec<Entity>,
    /// When line count changes, stores the line index from which all subsequent
    /// line entities should be invalidated.
    pub invalidate_lines_from: Option<usize>,
}

/// Bundled mutable refs to the editor's per-entity buffer state.
///
/// Editing operations (`insert_char`, `delete_*`, undo/redo, action dispatch)
/// always need the same six components together — `SelectionState`,
/// `EditHistoryState`, `SyntaxCacheState`, `EditorDisplayState`, `CursorState`,
/// `TextViewState`. This struct lets system functions accept them as one
/// `&mut EditorBuf<'_>` instead of six positional `&mut` parameters.
///
/// Helpers below the dispatcher level still take individual `&mut`s — the
/// goal here is to slim system signatures and the action dispatcher, not to
/// rewrite ~145 internal call sites.
pub struct EditorBuf<'a> {
    pub sel: &'a mut crate::types::SelectionState,
    pub hist: &'a mut crate::types::EditHistoryState,
    pub syntax: &'a mut SyntaxCacheState,
    pub display: &'a mut EditorDisplayState,
    pub cursor: &'a mut crate::types::CursorState,
    pub tv: &'a mut crate::text_view::TextViewState,
}

/// Component markers for editor entities

#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
pub struct EditorText;

#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct HighlightedTextToken {
    pub index: usize,
}

#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct EditorCursor {
    /// Index of this cursor in the cursors array (0 = primary cursor)
    pub cursor_index: usize,
}

#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
pub struct LineNumbers;

#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
pub struct Separator;

#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct SelectionHighlight {
    pub line_index: usize,
    /// Index of the cursor this selection belongs to (0 = primary cursor)
    pub cursor_index: usize,
}

/// Component marker for bracket match highlight entities (bounding box style)
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct BracketMatchHighlight {
    /// Which bracket this belongs to (0 = cursor bracket, 1 = matching bracket)
    pub bracket_index: usize,
    /// Which border edge (0=top, 1=bottom, 2=left, 3=right)
    pub edge: usize,
}

/// Component marker for current line border (top or bottom line)
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct CursorLineBorder {
    /// The cursor index this border belongs to (for multi-cursor support)
    pub cursor_index: usize,
    /// Whether this is the top (true) or bottom (false) border
    pub is_top: bool,
}

/// Component marker for current word highlight (under cursor)
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct CursorWordHighlight {
    /// The cursor index this highlight belongs to (for multi-cursor support)
    pub cursor_index: usize,
}

/// Component marker for indent guide entities
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct IndentGuide {
    /// The indentation level (0 = first indent, 1 = second indent, etc.)
    pub level: usize,
    /// The line index this guide is on
    pub line_index: usize,
}

/// Per-input-manager key-repeat state.
///
/// Attached to the same entity as `EditorInputManager`. `Instant` isn't
/// `Reflect`; only the action enum is observable through reflection.
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
pub struct KeyRepeatState {
    /// The action currently being repeated (if any)
    pub current_action: Option<crate::input::EditorAction>,
    /// When the action key was first pressed
    #[reflect(ignore)]
    pub press_start: Option<Instant>,
    /// When the last repeat occurred
    #[reflect(ignore)]
    pub last_repeat: Option<Instant>,
}

/// Represents a matched bracket pair
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[reflect(Debug, PartialEq)]
pub struct BracketMatch {
    /// Position of the bracket at/near cursor
    pub cursor_bracket_pos: usize,
    /// Position of the matching bracket
    pub matching_bracket_pos: usize,
}

/// Per-editor bracket-match state — the bracket pair under the cursor.
#[derive(Component, Default, Clone, Debug, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct BracketMatchState {
    /// Current bracket match (if any)
    pub current_match: Option<BracketMatch>,
}

/// Component marker for find/search highlight entities
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct FindHighlight {
    /// Index of this match in the matches list
    pub match_index: usize,
}

/// Event emitted when save is requested (Ctrl+S)
/// The host application should handle this event to save the buffer contents.
#[derive(bevy::prelude::Message, Clone, Debug, Reflect)]
#[reflect(Clone, Debug)]
pub struct SaveRequested {
    /// The current buffer content
    pub content: String,
}

/// Event emitted when open is requested (Ctrl+O)
/// The host application should handle this event to show a file picker.
#[derive(bevy::prelude::Message, Clone, Debug, Reflect, Default)]
#[reflect(Clone, Debug, Default)]
pub struct OpenRequested;
