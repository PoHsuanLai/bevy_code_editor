//! UI settings - visual elements and layout

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Per-editor UI visual settings. Layout dimensions are computed from these
/// and written into `Node::padding` each frame via `sync_gutter_width`.
#[derive(Component, Clone, Debug, Serialize, Deserialize, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct EditorUi {
    pub show_line_numbers: bool,
    /// Vim-style relative line numbers.
    pub relative_line_numbers: bool,
    pub show_gutter: bool,
    pub show_indent_guides: bool,
    pub show_whitespace: WhitespaceMode,
    pub highlight_active_line: bool,
    pub show_separator: bool,
    pub gutter_padding_left: f32,
    pub gutter_padding_right: f32,
    pub code_margin_left: f32,
    pub margin_top: f32,
}

/// Resolved gutter geometry, populated every frame by `sync_gutter_width`.
/// Read by line-number rendering and fold gutter hit-testing.
/// Do not set manually — it is derived from `EditorUi` + `TextFont`.
#[derive(Component, Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct GutterConfig {
    pub gutter_width: f32,
}

/// Controls which whitespace characters are rendered as visible glyphs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Debug, PartialEq)]
pub enum WhitespaceMode {
    /// No whitespace markers shown.
    None,
    /// Show markers only within the current selection.
    Selection,
    /// Show markers for trailing whitespace only.
    Trailing,
    /// Show all whitespace markers.
    All,
}

impl Default for EditorUi {
    fn default() -> Self {
        Self {
            show_line_numbers: true,
            relative_line_numbers: false,
            show_gutter: true,
            show_indent_guides: false,
            show_whitespace: WhitespaceMode::None,
            highlight_active_line: true,
            show_separator: true,
            gutter_padding_left: 10.0,
            gutter_padding_right: 10.0,
            code_margin_left: 10.0,
            margin_top: 10.0,
        }
    }
}

/// Per-editor indentation rules: spaces vs tabs, width, and auto-indent on newline.
#[derive(Component, Clone, Debug, Serialize, Deserialize, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct Indentation {
    pub use_spaces: bool,
    pub tab_width: usize,
    /// Alias of `tab_width` for compatibility.
    pub indent_size: usize,
    pub auto_indent: bool,
}

impl Default for Indentation {
    fn default() -> Self {
        Self {
            use_spaces: true,
            tab_width: 4,
            indent_size: 4,
            auto_indent: true,
        }
    }
}

/// Per-editor bracket matching and auto-close settings.
#[derive(Component, Clone, Debug, Serialize, Deserialize, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct BracketConfig {
    pub enabled: bool,
    pub style: BracketHighlightStyle,
    pub auto_close: bool,
    pub auto_close_quotes: bool,
    pub pairs: Vec<(char, char)>,
}

/// Visual style used to highlight a matched bracket pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Debug, PartialEq)]
pub enum BracketHighlightStyle {
    /// Underline both brackets.
    Underline,
    /// Background fill on both brackets.
    Background,
    /// Both underline and background fill.
    Both,
}

impl Default for BracketConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            style: BracketHighlightStyle::Background,
            auto_close: true,
            auto_close_quotes: true,
            pairs: vec![('(', ')'), ('[', ']'), ('{', '}'), ('<', '>')],
        }
    }
}
