//! Editor component types.

use bevy::prelude::*;

/// Marker component for a code editor entity.
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
#[require(
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
    bevy::input_focus::tab_navigation::TabIndex,
)]
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

#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct EditorCursor {
    /// 0 = primary cursor; higher indices are multi-cursor additions.
    pub cursor_index: usize,
}

#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
pub struct LineNumbers;

#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct GutterContainer {
    pub editor: Entity,
}

#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct GutterTextView {
    /// Editor entity this gutter belongs to.
    pub editor: Entity,
}

#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
pub struct Separator;

#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct SelectionHighlight {
    pub line_index: usize,
    /// Index of the cursor this selection belongs to (0 = primary cursor).
    pub cursor_index: usize,
}

#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct BracketMatchHighlight;

pub type KeyRepeatState = bevy_instanced_text_editor::KeyRepeatState<crate::input::EditorAction>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[reflect(Debug, PartialEq)]
pub struct BracketMatch {
    pub cursor_bracket_pos: usize,
    pub matching_bracket_pos: usize,
}

#[derive(Component, Default, Clone, Debug, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct BracketMatchState {
    pub current_match: Option<BracketMatch>,
}

use bevy_instanced_text::RectOverlay;

#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct SelectionRects(pub Vec<RectOverlay>);

#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct IndentGuideRects(pub Vec<RectOverlay>);

#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct RulerRects(pub Vec<RectOverlay>);

#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct FoldHighlightRects(pub Vec<RectOverlay>);

#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct CaretRects(pub Vec<RectOverlay>);

#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct CursorLineRects(pub Vec<RectOverlay>);

#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct BracketMatchRects(pub Vec<RectOverlay>);

#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct WhitespaceRects(pub Vec<RectOverlay>);

#[derive(bevy::prelude::Message, Clone, Debug, Reflect)]
#[reflect(Clone, Debug)]
pub struct SaveRequested {
    pub content: String,
}

#[derive(bevy::prelude::Message, Clone, Debug, Reflect, Default)]
#[reflect(Clone, Debug, Default)]
pub struct OpenRequested;
