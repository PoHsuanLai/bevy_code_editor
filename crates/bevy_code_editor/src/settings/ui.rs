//! UI settings - visual elements and layout

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// UI visual settings. Layout dimensions are computed by the UI plugin and
/// written into each editor's `TextViewViewport` component.
#[derive(Clone, Debug, Resource, Serialize, Deserialize, Reflect)]
#[reflect(Resource, Default, Debug)]
pub struct UiSettings {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Debug, PartialEq)]
pub enum WhitespaceMode {
    None,
    Selection,
    Trailing,
    All,
}

impl Default for UiSettings {
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

#[derive(Clone, Debug, Resource, Serialize, Deserialize, Reflect)]
#[reflect(Resource, Default, Debug)]
pub struct IndentationSettings {
    pub use_spaces: bool,
    pub tab_width: usize,
    /// Alias of `tab_width` for compatibility.
    pub indent_size: usize,
    pub auto_indent: bool,
}

impl Default for IndentationSettings {
    fn default() -> Self {
        Self {
            use_spaces: true,
            tab_width: 4,
            indent_size: 4,
            auto_indent: true,
        }
    }
}

#[derive(Clone, Debug, Resource, Serialize, Deserialize, Reflect)]
#[reflect(Resource, Default, Debug)]
pub struct BracketSettings {
    pub enabled: bool,
    pub style: BracketHighlightStyle,
    pub auto_close: bool,
    pub auto_close_quotes: bool,
    pub pairs: Vec<(char, char)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Debug, PartialEq)]
pub enum BracketHighlightStyle {
    Underline,
    Background,
    Both,
}

impl Default for BracketSettings {
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
