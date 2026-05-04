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

/// Font sizing for a single text view entity.
///
/// Monospace-only for now: `char_width` is a scalar advance applied to
/// every glyph. Phase 4 will add weight/family/decoration on `StyleRun`,
/// and a future phase will replace the scalar with shaped advances.
#[derive(Component, Clone, Debug)]
pub struct FontConfig {
    /// Glyph height in pixels.
    pub size: f32,
    /// Vertical distance between baselines, in pixels.
    pub line_height: f32,
    /// Monospace advance per character, in pixels.
    pub char_width: f32,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self::from_size(14.0)
    }
}

impl FontConfig {
    /// `size`-derived defaults: `line_height = size * 1.5`, `char_width = size * 0.6`.
    pub const fn from_size(size: f32) -> Self {
        Self {
            size,
            line_height: size * 1.5,
            char_width: size * 0.6,
        }
    }

    /// Override the line height with an explicit pixel value.
    pub const fn with_line_height(mut self, line_height: f32) -> Self {
        self.line_height = line_height;
        self
    }

    /// Override the line height as a multiple of `size`.
    pub const fn with_line_height_multiplier(mut self, multiplier: f32) -> Self {
        self.line_height = self.size * multiplier;
        self
    }

    /// Override the monospace character advance.
    pub const fn with_char_width(mut self, char_width: f32) -> Self {
        self.char_width = char_width;
        self
    }
}
