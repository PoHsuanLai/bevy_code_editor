//! Markdown-specific styling.

use bevy::prelude::*;
pub use bevy_instanced_text::BlockDecorTheme;

/// Syntax highlight color palette for fenced code blocks.
///
/// Maps tree-sitter capture categories to colors. Mirrors
/// `bevy_code_editor::SyntaxTheme` but lives here so `bevy_markdown` doesn't
/// depend on the editor crate.
#[derive(Clone, Debug)]
pub struct SyntaxPalette {
    pub keyword: Color,
    pub function: Color,
    pub type_name: Color,
    pub variable: Color,
    pub constant: Color,
    pub string: Color,
    pub comment: Color,
    pub operator: Color,
    pub punctuation: Color,
    pub property: Color,
    pub escape: Color,
    pub default: Color,
}

impl Default for SyntaxPalette {
    fn default() -> Self {
        Self {
            keyword:     Color::srgb(0.847, 0.486, 0.659),
            function:    Color::srgb(0.863, 0.863, 0.549),
            type_name:   Color::srgb(0.298, 0.686, 0.914),
            variable:    Color::srgb(0.608, 0.788, 0.933),
            constant:    Color::srgb(0.298, 0.686, 0.914),
            string:      Color::srgb(0.808, 0.616, 0.502),
            comment:     Color::srgb(0.384, 0.514, 0.376),
            operator:    Color::srgb(0.827, 0.827, 0.827),
            punctuation: Color::srgb(0.827, 0.827, 0.827),
            property:    Color::srgb(0.608, 0.788, 0.933),
            escape:      Color::srgb(0.863, 0.863, 0.549),
            default:     Color::srgb(0.90, 0.90, 0.92),
        }
    }
}

impl SyntaxPalette {
    pub fn color_for(&self, capture: &str) -> Color {
        let base = capture.split('.').next().unwrap_or(capture);
        match base {
            "keyword" | "conditional" | "repeat" | "exception" => self.keyword,
            "function" | "method" => self.function,
            "type" | "class" | "interface" | "struct" | "enum" | "namespace" | "module" => self.type_name,
            "variable" | "parameter" | "field" => self.variable,
            "constant" | "boolean" | "number" | "float" => self.constant,
            "string" | "character" => self.string,
            "comment" | "note" | "warning" | "danger" => self.comment,
            "operator" => self.operator,
            "punctuation" | "delimiter" | "bracket" | "special" => self.punctuation,
            "property" | "attribute" | "tag" | "decorator" => self.property,
            "constructor" | "label" => self.type_name,
            "escape" | "embedded" | "include" | "preproc" => self.escape,
            _ => self.default,
        }
    }
}

/// Markdown-specific palette: text colors, heading scales, indentation,
/// and per-element padding. Block chrome (inline-code chip, fenced-code
/// background, blockquote bar, rule color) lives in [`MarkdownTheme::decor`].
///
/// Per-entity Component: cascaded onto every `MarkdownDoc` via `#[require]`
/// so the simple case (one viewer) needs no extra spawn boilerplate. Hosts
/// override fields on a per-entity basis to restyle:
///
/// ```rust,ignore
/// commands.spawn((
///     MarkdownDoc::new(text),
///     MarkdownTheme {
///         body_fg: Color::srgb(0.1, 0.1, 0.1),
///         ..default()
///     },
///     // ...rest of the viewer bundle
/// ));
/// ```
#[derive(Clone, Debug, Component)]
pub struct MarkdownTheme {
    pub body_fg: Color,
    /// Heading colors indexed by `level - 1` (H1 at index 0).
    pub heading_fg: [Color; 6],
    /// Heading font scales relative to body, indexed by `level - 1`.
    pub heading_scale: [f32; 6],
    pub link_fg: Color,
    pub decor: BlockDecorTheme,
    /// Vertical padding (px) inside fenced code blocks (top and bottom).
    pub code_block_padding: f32,
    /// Horizontal text indent (px) inside fenced code blocks (left gutter breathing room).
    pub code_block_indent: f32,
    /// Horizontal indent (px) per nesting level for lists / blockquotes.
    pub indent_step: f32,
    pub heading_padding_top: f32,
    pub heading_padding_bottom: f32,
    pub paragraph_padding_bottom: f32,
    /// CSS-style font weight for `# Heading` runs (bold synthesis kicks
    /// in when no bold face is loaded). 700 = bold; raise for an
    /// extra-bold display, drop to 600 for a softer look.
    pub heading_weight: u16,
    /// CSS-style font weight for `**strong**` inline runs. 700 = bold,
    /// matching CSS user-agent defaults; can be set independently of
    /// `heading_weight` (e.g. heavier headings + standard inline bold).
    pub strong_weight: u16,
    /// Width (px) of the left bar drawn beside `> blockquote` blocks.
    pub blockquote_bar_width: f32,
    /// Stroke thickness (px) for `---` thematic-break rules.
    pub rule_thickness: f32,
    /// Padding (px) above and below `---` rules.
    pub rule_padding: f32,
    /// Syntax highlight colors for fenced code blocks (used when the
    /// `tree-sitter` feature is enabled; ignored otherwise).
    pub syntax: SyntaxPalette,
}

impl Default for MarkdownTheme {
    fn default() -> Self {
        Self {
            body_fg: Color::srgb(0.86, 0.86, 0.88),
            heading_fg: [
                Color::srgb(1.0, 1.0, 1.0),
                Color::srgb(0.96, 0.96, 0.98),
                Color::srgb(0.92, 0.92, 0.95),
                Color::srgb(0.88, 0.88, 0.92),
                Color::srgb(0.84, 0.84, 0.88),
                Color::srgb(0.80, 0.80, 0.85),
            ],
            heading_scale: [2.0, 1.6, 1.3, 1.15, 1.05, 1.0],
            link_fg: Color::srgb(0.45, 0.72, 1.0),
            decor: BlockDecorTheme::default(),
            code_block_padding: 16.0,
            code_block_indent: 24.0,
            indent_step: 24.0,
            heading_padding_top: 12.0,
            heading_padding_bottom: 6.0,
            paragraph_padding_bottom: 6.0,
            heading_weight: 700,
            strong_weight: 700,
            blockquote_bar_width: 4.0,
            rule_thickness: 1.0,
            rule_padding: 8.0,
            syntax: SyntaxPalette::default(),
        }
    }
}
