//! The `CodeEditor` marker and its required-component manifest.

use bevy::prelude::*;

/// Marker component for a code editor entity.
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
#[require(
    bevy_instanced_text_editor::TextEditor,
    crate::types::brackets::BracketMatchState,
    crate::types::goto_line::GotoLineState,
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
    crate::types::overlays::SelectionRects,
    crate::types::overlays::IndentGuideRects,
    crate::types::overlays::RulerRects,
    crate::types::overlays::FoldHighlightRects,
    crate::types::overlays::CaretRects,
    crate::types::overlays::CursorLineRects,
    crate::types::overlays::BracketMatchRects,
    crate::types::overlays::WhitespaceRects,
    crate::plugin::LinkRects,
    crate::plugin::LinkRanges,
    crate::plugin::HoveredLink,
    crate::plugin::GlyphMarkers,
    crate::plugin::GutterDecorations,
    crate::plugin::GlyphMarginRects,
    crate::plugin::LineDecorationRects,
    crate::types::gutter::HoveredGutterLine,
    crate::types::gutter::HoveredInGutter,
    bevy::input_focus::tab_navigation::TabIndex
)]
pub struct CodeEditor;
