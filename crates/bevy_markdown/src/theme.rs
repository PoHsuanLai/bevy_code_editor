//! Markdown-specific styling. Decorative chrome (code chips, blockquote
//! bars, rules) is delegated to [`bevy_text_engine::BlockDecorTheme`] so
//! terminals, log viewers, and other consumers can share it.

use bevy::prelude::*;
use bevy_text_engine::BlockDecorTheme;

/// Markdown-specific palette: text colors, heading scales, indentation,
/// and per-element padding. Generic block chrome (inline-code chip,
/// fenced-code background, blockquote bar, rule color) lives in
/// [`MarkdownTheme::decor`] as a [`BlockDecorTheme`].
#[derive(Clone, Debug, Resource)]
pub struct MarkdownTheme {
    pub body_fg: Color,
    /// Heading colors indexed by `level - 1` (H1 at index 0).
    pub heading_fg: [Color; 6],
    /// Heading font scales relative to body, indexed by `level - 1`.
    pub heading_scale: [f32; 6],
    pub link_fg: Color,
    pub decor: BlockDecorTheme,
    /// Padding (px) inside fenced code blocks.
    pub code_block_padding: f32,
    /// Horizontal indent (px) per nesting level for lists / blockquotes.
    pub indent_step: f32,
    pub heading_padding_top: f32,
    pub heading_padding_bottom: f32,
    pub paragraph_padding_bottom: f32,
}

impl Default for MarkdownTheme {
    fn default() -> Self {
        Self::dark()
    }
}

impl MarkdownTheme {
    /// Default dark theme — neutral grays + accent blue for links.
    pub fn dark() -> Self {
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
            code_block_padding: 8.0,
            indent_step: 24.0,
            heading_padding_top: 12.0,
            heading_padding_bottom: 6.0,
            paragraph_padding_bottom: 6.0,
        }
    }
}
