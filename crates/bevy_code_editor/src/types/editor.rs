//! Editor component types — pure data definitions, no logic.
//!
//! The editable-text Components (`CursorState`, `SelectionState`,
//! `EditHistoryState`) are defined in [`bevy_text_editor`] and re-exported
//! through `crate::types`. Operational helpers live in `input/`:
//! - `input/editor_ops.rs` — free fns for search and editor cursor movement
//! - `input/multi_cursor.rs` — multi-cursor add/remove

use bevy::prelude::*;


/// When `true`, viewport auto-resizes to window; when `false`, the host
/// writes directly to each editor's [`crate::text_view::TextViewViewport`]
/// component.
#[derive(Resource, Clone, Copy, Debug, Reflect)]
#[reflect(Resource, Default, Debug)]
pub struct ViewportConfig {
    pub auto_resize_to_window: bool,
}

impl Default for ViewportConfig {
    fn default() -> Self {
        Self {
            auto_resize_to_window: true,
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
        BracketMatchState,
        crate::types::fold::GotoLineState,
        crate::types::fold::FoldState,
        crate::plugin::scrollbar::ScrollbarDragState,
        crate::settings::ThemeConfig,
        crate::settings::SyntaxTheme,
    )
)]
#[cfg_attr(
    feature = "lsp",
    require(
        bevy_text_editor::TextEditor,
        BracketMatchState,
        crate::types::fold::GotoLineState,
        crate::types::fold::FoldState,
        crate::plugin::scrollbar::ScrollbarDragState,
        crate::settings::ThemeConfig,
        crate::settings::SyntaxTheme,
        crate::settings::DiagnosticTheme,
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
        crate::lsp_ui::state::TabstopSession,
    )
)]
pub struct CodeEditor;

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
pub struct BracketMatchHighlight;

/// Component marker for indent guide entities
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct IndentGuide {
    /// The indentation level (0 = first indent, 1 = second indent, etc.)
    pub level: usize,
    /// The line index this guide is on
    pub line_index: usize,
}

/// Per-input-manager key-repeat state for editor actions.
///
/// Re-export of the generic [`bevy_text_editor::KeyRepeatState`] specialized
/// to [`crate::input::EditorAction`]. Attached to the same entity as
/// `EditorInputManager`.
pub type KeyRepeatState = bevy_text_editor::KeyRepeatState<crate::input::EditorAction>;

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
