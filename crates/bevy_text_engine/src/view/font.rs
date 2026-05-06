//! Per-entity font configuration. The renderer reads this; there is no global
//! font resource.

use bevy::prelude::*;
use bevy::text::Font;

/// Font sizing + handles. The atlas registers handle bytes on first use.
///
/// Bold/italic slots are optional; missing ones fall back to the regular face
/// with synthesis (stroke-doubling for bold, skew for italic), matching the CSS
/// `font-synthesis: weight | style` defaults. `char_width` is a fallback
/// advance used when `shape: None` (the `trivial_layout` path).
#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct FontConfig {
    pub size: f32,
    pub line_height: f32,
    pub char_width: f32,
    pub font: Option<Handle<Font>>,
    pub font_bold: Option<Handle<Font>>,
    pub font_italic: Option<Handle<Font>>,
    pub font_bold_italic: Option<Handle<Font>>,
    pub font_synthesis: FontSynthesis,
}

/// Whether to synthesize a bold / italic face when the matching slot on
/// [`FontConfig`] is empty. Both default `true`, matching CSS Fonts L4
/// `font-synthesis: weight style`.
///
/// Disable per-axis when uniform-weight rendering reads better than a
/// blurry faux-bold (e.g. body text) — the renderer will draw the
/// regular face unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[reflect(Default, Debug)]
pub struct FontSynthesis {
    /// When `true` and no bold face is loaded, draw bold runs with a
    /// stroke-doubled regular face (faux bold).
    pub weight: bool,
    /// When `true` and no italic face is loaded, draw italic runs with
    /// a skew applied to the regular face (faux italic).
    pub style: bool,
}

impl Default for FontSynthesis {
    fn default() -> Self {
        Self {
            weight: true,
            style: true,
        }
    }
}

impl Default for FontConfig {
    fn default() -> Self {
        Self::from_size(14.0)
    }
}

impl FontConfig {
    /// `line_height = size * 1.5`, `char_width = size * 0.6`,
    /// `font = Handle::default()` (Bevy's FiraMono-subset when `default_font` is on).
    pub fn from_size(size: f32) -> Self {
        Self {
            size,
            line_height: size * 1.5,
            char_width: size * 0.6,
            font: Some(Handle::default()),
            font_bold: None,
            font_italic: None,
            font_bold_italic: None,
            font_synthesis: FontSynthesis::default(),
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

    pub fn with_bold_font(mut self, handle: Handle<Font>) -> Self {
        self.font_bold = Some(handle);
        self
    }

    pub fn with_italic_font(mut self, handle: Handle<Font>) -> Self {
        self.font_italic = Some(handle);
        self
    }

    pub fn with_bold_italic_font(mut self, handle: Handle<Font>) -> Self {
        self.font_bold_italic = Some(handle);
        self
    }

    pub fn with_font_synthesis(mut self, synthesis: FontSynthesis) -> Self {
        self.font_synthesis = synthesis;
        self
    }

    /// Resolve a handle for `(bold, italic)`, falling back to the closest
    /// available face. Caller applies synthesis when the regular face is
    /// returned for a styled request.
    pub fn font_for(&self, bold: bool, italic: bool) -> Option<&Handle<Font>> {
        match (bold, italic) {
            (true, true) => self
                .font_bold_italic
                .as_ref()
                .or(self.font_bold.as_ref())
                .or(self.font_italic.as_ref())
                .or(self.font.as_ref()),
            (true, false) => self.font_bold.as_ref().or(self.font.as_ref()),
            (false, true) => self.font_italic.as_ref().or(self.font.as_ref()),
            (false, false) => self.font.as_ref(),
        }
    }
}
