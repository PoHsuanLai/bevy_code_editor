//! Paint-time overlays: cursor, selection, line highlights, bracket matches.
//!
//! Overlays are decoration the editor (or any consumer) writes *alongside* the
//! display layout. The renderer reads them during the same pass and emits quads
//! into the same instance buffer as glyphs (sharing the atlas's `solid_uv`).
//!
//! Single-writer rule: each system that produces overlays must `clear()` first
//! and append, so the rect list rebuilds each frame. Bumping `version` skips
//! the GPU upload when nothing changed.

use bevy::prelude::*;
use std::ops::Range;

#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct TextViewOverlays {
    pub rects: Vec<RectOverlay>,
    pub version: u64,
}

impl TextViewOverlays {
    /// Reset for a fresh frame. Call once at the start of `OverlaySet`.
    pub fn clear(&mut self) {
        self.rects.clear();
        self.version = self.version.wrapping_add(1);
    }
}

/// A rectangle drawn anchored to a display row.
///
/// `display_row` indexes into `DisplayLayout.lines`; `x_range` is in pixels
/// relative to the row's text origin. `0.0..f32::MAX` covers the full line.
///
/// `vertical` declares *what kind* of decoration this is — `Full` for selection
/// backgrounds, `Caret` for cursors, `TopBand`/`BottomBand` for cursor-line
/// borders, `UnderBaseline` for underlines/squiggles. The renderer translates
/// these into pixels using the row's geometry. Producers never compute Y.
///
/// `corners` carries per-corner radii so multi-row block backgrounds
/// (the first row rounds top-left/top-right, the last row rounds
/// bottom-left/bottom-right, middle rows are sharp) read as a single
/// continuous panel. Use [`CornerRadii::uniform`] for the common case.
#[derive(Clone, Debug, Reflect)]
#[reflect(Debug)]
pub struct RectOverlay {
    pub display_row: u32,
    pub x_range: Range<f32>,
    pub vertical: RowVertical,
    pub color: Color,
    /// Z order: -1 = below text (selection bg, line highlight), +1 = above text (carets).
    pub z: i8,
    pub corners: CornerRadii,
}

/// Per-corner radii in pixels. `0.0` = sharp corner. The renderer's SDF
/// uses the matching radius for each quadrant of the quad, so a rect
/// with `tl = tr = R, bl = br = 0` rounds only its top corners — the
/// pattern needed for the first row of a multi-row code-block panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Default, Debug, PartialEq)]
pub struct CornerRadii {
    pub tl: f32,
    pub tr: f32,
    pub bl: f32,
    pub br: f32,
}

impl CornerRadii {
    pub const ZERO: Self = Self {
        tl: 0.0,
        tr: 0.0,
        bl: 0.0,
        br: 0.0,
    };

    /// All four corners share the same radius. Cursor / caret / single-row
    /// backgrounds use this; multi-row panels use the per-corner ctors below.
    pub const fn uniform(r: f32) -> Self {
        Self {
            tl: r,
            tr: r,
            bl: r,
            br: r,
        }
    }

    /// Round only the top corners (used on the first row of a multi-row
    /// block background).
    pub const fn top(r: f32) -> Self {
        Self {
            tl: r,
            tr: r,
            bl: 0.0,
            br: 0.0,
        }
    }

    /// Round only the bottom corners (used on the last row of a multi-row
    /// block background).
    pub const fn bottom(r: f32) -> Self {
        Self {
            tl: 0.0,
            tr: 0.0,
            bl: r,
            br: r,
        }
    }

    pub fn max(&self) -> f32 {
        self.tl.max(self.tr).max(self.bl).max(self.br)
    }
}

/// Semantic vertical placement within a row. Resolved to pixels by the renderer.
#[derive(Clone, Copy, Debug, Reflect)]
#[reflect(Debug)]
pub enum RowVertical {
    /// Span the row's typographic text band (cap-to-descender). Used
    /// for selection backgrounds and line-highlight bands so the rect
    /// hugs the visible text rather than straddling line-leading
    /// whitespace. Adjacent rows leave a small gap between bands —
    /// preferred when rects shouldn't visually merge across rows.
    Full,
    /// Span the row's full leaded height (`y_top .. y_top + line_height`).
    /// Used for multi-row block backgrounds (fenced code blocks,
    /// blockquotes) where adjacent rows should paint a continuous
    /// panel with no line-spacing gap between them.
    FullLeaded,
    /// Vertically centered on the row, at `height_fraction * line_height` tall.
    /// `1.0` = full row, `0.9` ≈ vscode-ish caret.
    Caret { height_fraction: f32 },
    /// Thin band along the row's top edge.
    TopBand { thickness: f32 },
    /// Thin band along the row's bottom edge.
    BottomBand { thickness: f32 },
    /// Underline below the typographic baseline (squiggle / error indicator).
    UnderBaseline { thickness: f32, gap: f32 },
}
