//! Editor component types — pure data definitions, no logic.
//!
//! The editable-text Components (`CursorState`, `SelectionState`,
//! `EditHistoryState`) are defined in [`bevy_instanced_text_editor`] and re-exported
//! through `crate::types`. Operational helpers live in `input/`:
//! - `input/editor_ops.rs` — free fns for search and editor cursor movement
//! - `input/multi_cursor.rs` — multi-cursor add/remove

use bevy::prelude::*;

/// Marker component for a code editor entity.
///
/// `#[require]` cascades [`bevy_instanced_text_editor::TextEditor`] (which transitively
/// brings the engine `TextView`, cursor / selection / edit-history state,
/// and pointer-interaction state) plus the IDE-specific Components — fold
/// state, bracket matching, syntax cache, goto-line dialog, LSP UI state.
/// Spawning a `CodeEditor` is sufficient for a fully functional editor
/// entity.
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
#[cfg_attr(
    not(feature = "lsp"),
    require(
        bevy_instanced_text_editor::TextEditor,
        BracketMatchState,
        crate::types::fold::GotoLineState,
        crate::settings::EditorTheme,
        crate::settings::SyntaxColors,
        crate::settings::EditorUi,
        crate::settings::GutterConfig,
        crate::settings::Indentation,
        crate::settings::BracketConfig,
        crate::settings::CursorLine,
        crate::settings::Performance,
        crate::settings::Wrapping,
        crate::settings::Guides,
        crate::settings::Padding,
        crate::settings::Rulers,
        crate::settings::Minimap,
        crate::settings::StickyScroll,
        crate::settings::RenderSettings,
        crate::settings::Folding,
        crate::settings::AutoEdit,
        crate::settings::SelectionConfig,
        crate::settings::Find,
        crate::settings::Misc,
    )
)]
#[cfg_attr(
    feature = "lsp",
    require(
        bevy_instanced_text_editor::TextEditor,
        BracketMatchState,
        crate::types::fold::GotoLineState,
        crate::settings::EditorTheme,
        crate::settings::SyntaxColors,
        crate::settings::DiagnosticColors,
        crate::settings::EditorUi,
        crate::settings::GutterConfig,
        crate::settings::Indentation,
        crate::settings::BracketConfig,
        crate::settings::CursorLine,
        crate::settings::Performance,
        crate::settings::Wrapping,
        crate::settings::Guides,
        crate::settings::Padding,
        crate::settings::Rulers,
        crate::settings::Minimap,
        crate::settings::StickyScroll,
        crate::settings::RenderSettings,
        crate::settings::Folding,
        crate::settings::AutoEdit,
        crate::settings::SelectionConfig,
        crate::settings::Find,
        crate::settings::Misc,
        crate::settings::Suggest,
        crate::settings::LspConfig,
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
        crate::lsp_ui::state::LspDidChangeBatcher,
        crate::lsp_ui::state::TabstopSession,
        // Dismiss-side lifecycle Components shared by every popup
        // (request_id, popup chrome ref, pointer-in-popup tracker,
        // dismiss-grace timer, hot zone). One per popup kind.
        crate::lsp_ui::state::HoverLifecycle,
        crate::lsp_ui::state::CompletionLifecycle,
        crate::lsp_ui::state::SignatureLifecycle,
        crate::lsp_ui::state::CodeActionsLifecycle,
        crate::lsp_ui::state::RenameLifecycle,
    )
)]
#[require(
    crate::types::fold::FoldState,
    SelectionRects,
    IndentGuideRects,
    RulerRects,
    FoldHighlightRects,
    CaretRects,
    CursorLineRects,
    BracketMatchRects,
    WhitespaceRects,
    crate::plugin::LinkRects,
    crate::plugin::LinkRanges,
    crate::plugin::HoveredLink,
    crate::plugin::GlyphMarkers,
    crate::plugin::GutterDecorations,
    crate::plugin::GlyphMarginRects,
    crate::plugin::LineDecorationRects,
    HoveredGutterLine,
    HoveredInGutter,
    // Mark the editor focusable for `bevy_input_focus`. `TabNavigationPlugin`
    // (registered by every tempera widget plugin) installs an `acquire_focus`
    // observer that bubbles up from the click target: it stops at the first
    // entity with `TabIndex` and clears focus on a bare `Window`. Without
    // this, a click on the editor would land on the window and clear focus,
    // so keystrokes would no longer be delivered to `bevy_instanced_text_editor`'s
    // edit handlers (which gate on `InputFocus.get()`).
    bevy::input_focus::tab_navigation::TabIndex
)]
#[cfg_attr(feature = "lsp", require(crate::plugin::DiagnosticUnderlineRects))]
pub struct CodeEditor;

/// Buffer line currently under the pointer, used by gutter chevrons under
/// `Folding::show_controls::Mouseover`. `None` when the pointer is outside
/// the editor or has not moved since the last frame.
#[derive(Component, Default, Clone, Copy, Reflect)]
#[reflect(Component, Default)]
pub struct HoveredGutterLine(pub Option<usize>);

/// `true` when the pointer is over the gutter strip (line numbers,
/// chevrons), not the text area. Drives `sync_cursor_icon` so the OS
/// arrow shows over the gutter and the I-beam over text — matching
/// VSCode / Sublime behavior.
#[derive(Component, Default, Clone, Copy, Reflect)]
#[reflect(Component, Default)]
pub struct HoveredInGutter(pub bool);

/// Marker for a cursor sprite entity. `cursor_index` 0 is the primary cursor;
/// higher indices are additional cursors added via multi-cursor commands.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct EditorCursor {
    /// Index of this cursor in the cursors array (0 = primary cursor).
    pub cursor_index: usize,
}

/// Marker for line-number gutter text entities.
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
pub struct LineNumbers;

/// Marker for the gutter container Node that wraps every gutter
/// decoration (line numbers, glyph margin icons, line-decoration bars,
/// fold chevrons). Width tracks `GutterConfig::gutter_width`; children
/// position themselves relative to this Node (not the editor) so Taffy
/// clips them to the gutter region. One per editor.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct GutterContainer {
    pub editor: Entity,
}

/// Marker for the `TextView` entity that renders line numbers in the
/// gutter. Spawned as a child of [`GutterContainer`]; managed by
/// `sync_gutter_text_view`.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct GutterTextView {
    /// Editor entity this gutter belongs to.
    pub editor: Entity,
}

/// Marker for the vertical separator sprite between the gutter and the code area.
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
pub struct Separator;

/// Marker for a per-line selection highlight rectangle.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct SelectionHighlight {
    pub line_index: usize,
    /// Index of the cursor this selection belongs to (0 = primary cursor).
    pub cursor_index: usize,
}

/// Component marker for bracket match highlight entities (bounding box style)
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct BracketMatchHighlight;

/// Per-input-manager key-repeat state for editor actions.
///
/// Re-export of the generic [`bevy_instanced_text_editor::KeyRepeatState`] specialized
/// to [`crate::input::EditorAction`]. Attached to the same entity as
/// `EditorInputManager`.
pub type KeyRepeatState = bevy_instanced_text_editor::KeyRepeatState<crate::input::EditorAction>;

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

// ── Per-producer overlay components ─────────────────────────────────────────
// Each system writes only to its own component. The engine reads `TextUnderlays`
// and `TextOverlays` from `bevy_instanced_text`; a merge system in EditorUiPlugin
// assembles these into those two engine components each frame (only when changed).

use bevy_instanced_text::RectOverlay;

/// Selection background rects — written by `update_selection_highlight`.
#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct SelectionRects(pub Vec<RectOverlay>);

/// Indent guide rects — written by `update_indent_guides`.
#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct IndentGuideRects(pub Vec<RectOverlay>);

/// Vertical ruler rects — written by `update_rulers`.
#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct RulerRects(pub Vec<RectOverlay>);

/// Folded-region underline rects — written by `update_fold_highlights`.
#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct FoldHighlightRects(pub Vec<RectOverlay>);

/// Caret rects — written by `push_cursor_overlays`.
#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct CaretRects(pub Vec<RectOverlay>);

/// Cursor-line border and word-highlight rects — written by `update_cursor_line_highlight`.
#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct CursorLineRects(pub Vec<RectOverlay>);

/// Bracket match rects — written by `update_bracket_highlight`.
#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct BracketMatchRects(pub Vec<RectOverlay>);

/// Whitespace marker rects (centered dots for spaces, thin bars for tabs)
/// — written by `update_whitespace_markers`.
#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct WhitespaceRects(pub Vec<RectOverlay>);

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
