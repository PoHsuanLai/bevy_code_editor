//! Gutter + line-number + indentation + bracket-match settings.
//!
//! Monaco parity:
//! - `EditorUi` → `lineNumbers`, `lineNumbersMinChars`, `glyphMargin`,
//!   `lineDecorationsWidth`, `selectOnLineNumbers`, `placeholder`.
//! - `Indentation` → `tabSize`, `insertSpaces`, `detectIndentation`,
//!   `indentSize`, `useTabStops`, `stickyTabStops`, `trimWhitespaceOnDelete`.
//! - `BracketConfig` → `matchBrackets`, `bracketPairColorization`.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Debug, Serialize, Deserialize, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct EditorUi {
    pub line_numbers: LineNumbers,
    pub line_numbers_min_chars: u32,
    pub glyph_margin: bool,
    /// Width in pixels of the glyph-margin column when `glyph_margin` is
    /// true. Sized to fit a small dot / icon — Monaco's default is 16.
    pub glyph_margin_width: f32,
    /// Width of the line-decorations strip (VCS bars, severity bars).
    /// `0.0` disables the column.
    pub line_decorations_width: f32,
    pub select_on_line_numbers: bool,
    pub show_gutter: bool,
    pub show_separator: bool,
    pub gutter_padding_left: f32,
    pub gutter_padding_right: f32,
    pub code_margin_left: f32,
    pub placeholder: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Debug, PartialEq)]
pub enum LineNumbers {
    #[default]
    On,
    Off,
    Relative,
    Interval,
}

/// Resolved gutter geometry, populated every frame by `sync_gutter_width`.
/// Read by line-number rendering and fold gutter hit-testing.
/// Do not set manually — it is derived from `EditorUi` + `TextFont`.
///
/// Column layout (left to right):
/// `[ line-decorations | line-numbers | glyph-margin | fold-chevrons | text ]`
///
/// `*_x` fields are the left edge of each sub-band relative to the gutter
/// left edge. `*_width` is each band's width. Hosts/observers walk these
/// to position overlays (negative `x_range` in the overlay coordinate
/// space puts them in the gutter) or to hit-test clicks per band.
#[derive(Component, Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct GutterConfig {
    pub gutter_width: f32,
    pub line_decorations_x: f32,
    pub line_decorations_width: f32,
    pub line_numbers_x: f32,
    pub line_numbers_width: f32,
    pub glyph_margin_x: f32,
    pub glyph_margin_width: f32,
    pub chevron_x: f32,
    pub chevron_width: f32,
}

impl Default for EditorUi {
    fn default() -> Self {
        // Monaco/VSCode defaults — bands are flush with no outer
        // padding and no inter-band gap. Layout (left → right):
        //   [ glyph-margin | line-numbers | line-decorations | text ]
        Self {
            line_numbers: LineNumbers::On,
            line_numbers_min_chars: 2,
            glyph_margin: true,
            glyph_margin_width: 16.0,
            line_decorations_width: 10.0,
            select_on_line_numbers: true,
            show_gutter: true,
            show_separator: true,
            gutter_padding_left: 0.0,
            gutter_padding_right: 0.0,
            code_margin_left: 0.0,
            placeholder: None,
        }
    }
}

#[derive(Component, Clone, Debug, Serialize, Deserialize, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct Indentation {
    pub tab_size: u32,
    pub insert_spaces: bool,
    pub detect_indentation: bool,
    pub indent_size: IndentSize,
    pub use_tab_stops: bool,
    pub sticky_tab_stops: bool,
    pub trim_whitespace_on_delete: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Debug, PartialEq)]
pub enum IndentSize {
    #[default]
    TabSize,
    Cells(u32),
}

impl IndentSize {
    /// Resolve to a concrete column count, falling back to `tab_size`
    /// for [`IndentSize::TabSize`].
    pub fn resolve(self, tab_size: u32) -> usize {
        match self {
            Self::TabSize => tab_size as usize,
            Self::Cells(n) => n as usize,
        }
    }
}

impl Default for Indentation {
    fn default() -> Self {
        Self {
            tab_size: 4,
            insert_spaces: true,
            detect_indentation: true,
            indent_size: IndentSize::TabSize,
            use_tab_stops: true,
            sticky_tab_stops: false,
            trim_whitespace_on_delete: false,
        }
    }
}

#[derive(Component, Clone, Debug, Serialize, Deserialize, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct BracketConfig {
    pub match_brackets: MatchBrackets,
    pub style: BracketHighlightStyle,
    pub colorization: BracketPairColorization,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Debug, PartialEq)]
pub enum MatchBrackets {
    Never,
    Near,
    #[default]
    Always,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Debug, PartialEq)]
pub enum BracketHighlightStyle {
    Underline,
    #[default]
    Background,
    Both,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Reflect)]
#[reflect(Debug)]
pub struct BracketPairColorization {
    pub enabled: bool,
    pub independent_color_pool_per_type: bool,
}

impl Default for BracketPairColorization {
    fn default() -> Self {
        Self {
            enabled: true,
            independent_color_pool_per_type: false,
        }
    }
}

impl Default for BracketConfig {
    fn default() -> Self {
        Self {
            match_brackets: MatchBrackets::Always,
            style: BracketHighlightStyle::Background,
            colorization: BracketPairColorization::default(),
        }
    }
}
