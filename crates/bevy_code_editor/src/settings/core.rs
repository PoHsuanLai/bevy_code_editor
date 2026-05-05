//! Per-entity editor theme.
//!
//! Theme is a per-entity Component, not a global Resource: multi-editor
//! apps can run a dark editor next to a light one without splitting state.
//! Bevy's `#[require]` cascade attaches `ThemeConfig::default()` to every
//! `CodeEditor` entity, so spawning works without specifying colors.
//!
//! For different colors, override at spawn time
//! (`(CodeEditor, ThemeConfig { background: ..., ..default() })`) or
//! mutate the Component at runtime via `Query<&mut ThemeConfig, With<CodeEditor>>`.
//!
//! Syntax-coloring lives on a sibling `SyntaxTheme` Component (cfg
//! `tree-sitter`); LSP diagnostic colors on `DiagnosticTheme` (cfg `lsp`).
//! Both are also `#[require]`d by `CodeEditor` so they default in.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Per-entity editor color palette.
#[derive(Component, Clone, Debug, Serialize, Deserialize, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct ThemeConfig {
    /// Background color (camera clear color in standalone mode).
    pub background: Color,
    /// Default text color.
    pub foreground: Color,
    /// Cursor caret color.
    pub cursor: Color,
    /// Selection background.
    pub selection_background: Color,
    /// Current-line highlight band. `None` disables the band.
    pub line_highlight: Option<Color>,
    /// Gutter line numbers.
    pub line_numbers: Color,
    /// Active (cursor) line number.
    pub line_numbers_active: Color,
    /// Separator line between gutter and code.
    pub separator: Color,
    /// Indent guide vertical lines.
    pub indent_guide: Color,
    /// Matching bracket highlight.
    pub bracket_match: Color,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            background: Color::srgb(0.117, 0.117, 0.117),
            foreground: Color::srgb(0.827, 0.827, 0.827),
            cursor: Color::srgb(0.933, 0.933, 0.933),
            selection_background: Color::srgba(0.231, 0.373, 0.604, 0.4),
            line_highlight: Some(Color::srgba(0.2, 0.2, 0.2, 0.5)),
            line_numbers: Color::srgb(0.545, 0.545, 0.545),
            line_numbers_active: Color::srgb(0.827, 0.827, 0.827),
            separator: Color::srgb(0.2, 0.2, 0.2),
            indent_guide: Color::srgba(0.4, 0.4, 0.4, 0.2),
            bracket_match: Color::srgba(0.0, 1.0, 0.5, 0.3),
        }
    }
}

/// Per-entity LSP diagnostic colors. Cfg-gated on the `lsp` feature.
#[cfg(feature = "lsp")]
#[derive(Component, Clone, Debug, Serialize, Deserialize, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct DiagnosticTheme {
    pub error: Color,
    pub warning: Color,
    pub info: Color,
    pub hint: Color,
}

#[cfg(feature = "lsp")]
impl Default for DiagnosticTheme {
    fn default() -> Self {
        Self {
            error: Color::srgb(0.976, 0.298, 0.298),
            warning: Color::srgb(0.804, 0.667, 0.0),
            info: Color::srgb(0.294, 0.678, 0.961),
            hint: Color::srgb(0.675, 0.675, 0.675),
        }
    }
}
