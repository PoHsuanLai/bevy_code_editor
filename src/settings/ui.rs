//! UI settings - visual elements and layout

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// UI settings for visual elements
///
/// Note: Layout dimensions (margins, positions) are computed by the UI plugin
/// and stored in the `ViewportDimensions` resource for decoupling.
#[derive(Clone, Debug, Resource, Serialize, Deserialize)]
pub struct UiSettings {
    /// Show line numbers
    pub show_line_numbers: bool,

    /// Show relative line numbers (vim-style)
    pub relative_line_numbers: bool,

    /// Show gutter (area for line numbers, breakpoints, etc.)
    pub show_gutter: bool,

    /// Show indent guides
    pub show_indent_guides: bool,

    /// Show whitespace characters
    pub show_whitespace: WhitespaceMode,

    /// Highlight current line
    pub highlight_active_line: bool,

    /// Show separator line between gutter and code
    pub show_separator: bool,

    // UI plugin uses these preferences to compute ViewportDimensions layout
    /// Gutter padding left (pixels)
    pub gutter_padding_left: f32,

    /// Gutter padding right (pixels)
    pub gutter_padding_right: f32,

    /// Code margin left (pixels) - space between separator and code
    pub code_margin_left: f32,

    /// Top margin (pixels)
    pub margin_top: f32,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

/// Indentation settings
#[derive(Clone, Debug, Resource, Serialize, Deserialize)]
pub struct IndentationSettings {
    /// Use spaces instead of tabs
    pub use_spaces: bool,

    /// Tab width in characters
    pub tab_width: usize,

    /// Indent size (alias for tab_width for compatibility)
    pub indent_size: usize,

    /// Auto-indent on newline
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

/// Bracket matching settings
#[derive(Clone, Debug, Resource, Serialize, Deserialize)]
pub struct BracketSettings {
    /// Enable bracket matching
    pub enabled: bool,

    /// Highlight style
    pub style: BracketHighlightStyle,

    /// Auto-close brackets
    pub auto_close: bool,

    /// Auto-close quotes
    pub auto_close_quotes: bool,

    /// Bracket pairs
    pub pairs: Vec<(char, char)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
            pairs: vec![
                ('(', ')'),
                ('[', ']'),
                ('{', '}'),
                ('<', '>'),
            ],
        }
    }
}
