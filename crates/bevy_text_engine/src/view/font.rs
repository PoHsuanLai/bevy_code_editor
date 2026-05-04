//! Per-entity font configuration for text views.
//!
//! Mirror of `bevy_text::TextFont`. The renderer reads this off each entity
//! it draws; there is no global font resource.
//!
//! ```rust,ignore
//! use bevy_text_engine::FontConfig;
//!
//! commands.spawn((
//!     TextView,
//!     FontConfig::from_size(18.0).with_line_height_multiplier(1.4),
//! ));
//! ```

use bevy::prelude::*;
use bevy::text::Font;

/// Font sizing + optional `bevy_text::Font` handle. The atlas registers
/// the handle's bytes into its cosmic-text font system on first use, so
/// the same `asset_server.load("foo.ttf")` works in both `Text2d` and
/// `TextView`. `font: None` falls back to system fonts.
///
/// `char_width` is a scalar fallback advance — the renderer prefers
/// per-glyph shaped advances from `LineShape.glyphs[*].x` and only
/// falls back to the scalar for `trivial_layout` consumers shipping
/// `shape: None`.
#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct FontConfig {
    pub size: f32,
    pub line_height: f32,
    pub char_width: f32,
    pub font: Option<Handle<Font>>,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self::from_size(14.0)
    }
}

impl FontConfig {
    /// `size`-derived defaults: `line_height = size * 1.5`, `char_width = size * 0.6`.
    pub fn from_size(size: f32) -> Self {
        Self {
            size,
            line_height: size * 1.5,
            char_width: size * 0.6,
            font: None,
        }
    }

    pub fn with_line_height(mut self, line_height: f32) -> Self {
        self.line_height = line_height;
        self
    }

    pub fn with_line_height_multiplier(mut self, multiplier: f32) -> Self {
        self.line_height = self.size * multiplier;
        self
    }

    pub fn with_char_width(mut self, char_width: f32) -> Self {
        self.char_width = char_width;
        self
    }

    pub fn with_font(mut self, handle: Handle<Font>) -> Self {
        self.font = Some(handle);
        self
    }
}
