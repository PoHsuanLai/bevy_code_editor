//! Per-entity editor theme.
//!
//! Theme is a per-entity Component, not a global Resource: multi-editor
//! apps can run a dark editor next to a light one without splitting state.
//! Bevy's `#[require]` cascade attaches `EditorTheme::default()` to every
//! `CodeEditor` entity, so spawning works without specifying colors.
//!
//! For different colors, override at spawn time
//! (`(CodeEditor, EditorTheme { background: ..., ..default() })`) or
//! mutate the Component at runtime via `Query<&mut EditorTheme, With<CodeEditor>>`.
//!
//! Syntax-coloring lives on a sibling `SyntaxColors` Component (cfg
//! `tree-sitter`); LSP diagnostic colors on `DiagnosticColors` (cfg `lsp`).
//! Both are also `#[require]`d by `CodeEditor` so they default in.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Per-entity editor color palette.
#[derive(Component, Clone, Debug, Serialize, Deserialize, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct EditorTheme {
    pub background: Color,
    pub foreground: Color,
    pub cursor: Color,
    pub selection_background: Color,
    /// `None` disables the current-line highlight band.
    pub line_highlight: Option<Color>,
    pub line_numbers: Color,
    pub line_numbers_active: Color,
    pub separator: Color,
    pub indent_guide: Color,
    pub bracket_match: Color,
    /// Rotating palette for bracket-pair colorization.
    pub bracket_pair_colors: Vec<Color>,
    pub placeholder_color: Color,
    /// Underline color drawn on the last visible row of a folded region
    /// when `Folding::highlight` is true.
    pub fold_marker: Color,
}

impl Default for EditorTheme {
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
            bracket_pair_colors: vec![
                Color::srgb(0.86, 0.86, 0.26),
                Color::srgb(0.85, 0.42, 0.85),
                Color::srgb(0.20, 0.74, 0.91),
                Color::srgb(0.96, 0.55, 0.24),
                Color::srgb(0.40, 0.83, 0.40),
                Color::srgb(0.93, 0.36, 0.39),
            ],
            placeholder_color: Color::srgba(0.5, 0.5, 0.5, 0.6),
            fold_marker: Color::srgba(0.5, 0.5, 0.5, 0.5),
        }
    }
}

/// Per-entity LSP diagnostic colors. Cfg-gated on the `lsp` feature.
#[cfg(feature = "lsp")]
#[derive(Component, Clone, Debug, Serialize, Deserialize, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct DiagnosticColors {
    pub error: Color,
    pub warning: Color,
    pub info: Color,
    pub hint: Color,
}

#[cfg(feature = "lsp")]
impl Default for DiagnosticColors {
    fn default() -> Self {
        Self {
            error: Color::srgb(0.976, 0.298, 0.298),
            warning: Color::srgb(0.804, 0.667, 0.0),
            info: Color::srgb(0.294, 0.678, 0.961),
            hint: Color::srgb(0.675, 0.675, 0.675),
        }
    }
}
