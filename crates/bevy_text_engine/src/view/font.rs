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

/// Font sizing + `bevy_text::Font` handle. The atlas registers the
/// handle's bytes into its cosmic-text font system on first use, so the
/// same `asset_server.load("foo.ttf")` works in both `Text2d` and
/// `TextView`.
///
/// `font` defaults to `Handle::default()`, which Bevy's `TextPlugin`
/// (when its `default_font` feature is on, the default for the `bevy`
/// crate) populates with FiraMono-subset. Hosts that want a different
/// font set it explicitly via [`Self::with_font`]. Setting `font: None`
/// falls back to whatever the cosmic-text `FontSystem` discovers in
/// system fonts.
///
/// Bold / italic / bold-italic faces are optional. When a `StyleRun`
/// asks for bold (`font_weight >= 600`) or italic, the renderer picks
/// the matching slot here; if it's empty it falls back to the regular
/// face and (subject to `font_synthesis`) synthesizes the missing axis
/// — bold via stroke-doubling, italic via skew. Matches the CSS
/// `font-synthesis: weight | style` defaults.
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
    /// `size`-derived defaults: `line_height = size * 1.5`,
    /// `char_width = size * 0.6`, `font = Handle::default()` (resolves to
    /// Bevy's slotted FiraMono-subset when `bevy_text`'s `default_font`
    /// feature is enabled). Bold / italic slots start empty — callers
    /// that want real bold/italic rendering load the matching faces
    /// and set them via the `with_*` builders.
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

    /// Resolve the font handle for a `(bold, italic)` request, falling
    /// back when the matching slot is empty. The caller (renderer) is
    /// responsible for applying synthesis when the returned handle is
    /// the regular face but a styled face was asked for.
    ///
    /// Fallback order picks the closest face on the requested axis,
    /// trading off style first (bold-italic missing → bold), and only
    /// dropping all the way to regular when nothing styled exists.
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
