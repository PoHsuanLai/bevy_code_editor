//! Core editor settings: Theme + the default font path.
//!
//! Font sizing / handle / line-height live on the per-entity
//! [`bevy_text_engine::FontConfig`] Component (multi-editor with
//! different fonts requires per-entity, not global). The only
//! genuinely global font-related thing is "where does the default
//! font come from" — captured here as [`EditorDefaultFont`] so a
//! host can override the asset path before `CodeEditorPlugin` adds.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Asset path of the default font loaded into every `CodeEditor`
/// entity that doesn't already have a `FontConfig.font` handle.
///
/// Hosts override by inserting this resource before adding
/// `CodeEditorPlugin`:
/// ```rust,ignore
/// app.insert_resource(EditorDefaultFont {
///     family: "fonts/JetBrainsMono-Regular.ttf".to_string(),
/// });
/// ```
#[derive(Clone, Debug, Resource, Serialize, Deserialize, Reflect)]
#[reflect(Resource, Default, Debug)]
pub struct EditorDefaultFont {
    pub family: String,
}

impl Default for EditorDefaultFont {
    fn default() -> Self {
        Self {
            family: "fonts/FiraMono-Regular.ttf".to_string(),
        }
    }
}

/// Theme settings - colors for all UI elements
#[derive(Clone, Debug, Resource, Serialize, Deserialize, Reflect)]
#[reflect(Resource, Default, Debug)]
pub struct ThemeSettings {
    /// Background color
    pub background: Color,

    /// Text color (default)
    pub foreground: Color,

    /// Cursor color
    pub cursor: Color,

    /// Selection background
    pub selection_background: Color,

    /// Selection foreground (optional)
    pub selection_foreground: Option<Color>,

    /// Line highlight (current line)
    pub line_highlight: Option<Color>,

    /// Line numbers color
    pub line_numbers: Color,

    /// Active line number color
    pub line_numbers_active: Color,

    /// Gutter background
    pub gutter_background: Color,

    /// Separator line color
    pub separator: Color,

    /// Indent guide line color
    pub indent_guide: Color,

    /// Matching bracket highlight color
    pub bracket_match: Color,

    /// Find/search match highlight color
    pub find_match: Color,

    /// Current find match highlight color (the selected one)
    pub find_match_current: Color,

    /// Syntax highlighting colors
    #[cfg(feature = "tree-sitter")]
    pub syntax: crate::settings::SyntaxTheme,

    /// Diagnostic colors (errors, warnings)
    #[cfg(feature = "lsp")]
    pub diagnostics: DiagnosticColors,
}

#[cfg(feature = "lsp")]
#[derive(Clone, Debug, Serialize, Deserialize, Reflect, Default)]
#[reflect(Default, Debug)]
pub struct DiagnosticColors {
    pub error: Color,
    pub warning: Color,
    pub info: Color,
    pub hint: Color,
}

impl ThemeSettings {
    pub fn vscode_dark() -> Self {
        Self {
            background: Color::srgb(0.117, 0.117, 0.117),
            foreground: Color::srgb(0.827, 0.827, 0.827),
            cursor: Color::srgb(0.933, 0.933, 0.933),
            selection_background: Color::srgba(0.231, 0.373, 0.604, 0.4),
            selection_foreground: None,
            line_highlight: Some(Color::srgba(0.2, 0.2, 0.2, 0.5)),
            line_numbers: Color::srgb(0.545, 0.545, 0.545),
            line_numbers_active: Color::srgb(0.827, 0.827, 0.827),
            gutter_background: Color::srgb(0.098, 0.098, 0.098),
            separator: Color::srgb(0.2, 0.2, 0.2),
            indent_guide: Color::srgba(0.4, 0.4, 0.4, 0.2),
            bracket_match: Color::srgba(0.0, 1.0, 0.5, 0.3),
            find_match: Color::srgba(1.0, 1.0, 0.0, 0.3),
            find_match_current: Color::srgba(1.0, 0.647, 0.0, 0.5),

            #[cfg(feature = "tree-sitter")]
            syntax: crate::settings::SyntaxTheme::default(),

            #[cfg(feature = "lsp")]
            diagnostics: DiagnosticColors {
                error: Color::srgb(0.976, 0.298, 0.298),
                warning: Color::srgb(0.804, 0.667, 0.0),
                info: Color::srgb(0.294, 0.678, 0.961),
                hint: Color::srgb(0.675, 0.675, 0.675),
            },
        }
    }

    pub fn vscode_light() -> Self {
        Self {
            background: Color::srgb(1.0, 1.0, 1.0),
            foreground: Color::srgb(0.0, 0.0, 0.0),
            cursor: Color::srgb(0.0, 0.0, 0.0),
            selection_background: Color::srgba(0.678, 0.847, 1.0, 0.4),
            selection_foreground: None,
            line_highlight: Some(Color::srgba(0.95, 0.95, 0.95, 0.5)),
            line_numbers: Color::srgb(0.588, 0.588, 0.588),
            line_numbers_active: Color::srgb(0.0, 0.0, 0.0),
            gutter_background: Color::srgb(0.95, 0.95, 0.95),
            separator: Color::srgb(0.85, 0.85, 0.85),
            indent_guide: Color::srgba(0.6, 0.6, 0.6, 0.2),
            bracket_match: Color::srgba(0.0, 0.8, 0.4, 0.3),
            find_match: Color::srgba(0.9, 0.9, 0.0, 0.3),
            find_match_current: Color::srgba(1.0, 0.647, 0.0, 0.5),

            #[cfg(feature = "tree-sitter")]
            syntax: crate::settings::SyntaxTheme::default(),

            #[cfg(feature = "lsp")]
            diagnostics: DiagnosticColors {
                error: Color::srgb(0.937, 0.0, 0.0),
                warning: Color::srgb(0.804, 0.667, 0.0),
                info: Color::srgb(0.0, 0.478, 0.804),
                hint: Color::srgb(0.4, 0.4, 0.4),
            },
        }
    }
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self::vscode_dark()
    }
}
