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

#[derive(Component, Default, Clone)]
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
#[derive(Clone, Debug)]
pub struct RectOverlay {
    pub display_row: u32,
    pub x_range: Range<f32>,
    pub vertical: RowVertical,
    pub color: Color,
    /// Z order: -1 = below text (selection bg, line highlight), +1 = above text (carets).
    pub z: i8,
    pub corner_radius: f32,
}

/// Semantic vertical placement within a row. Resolved to pixels by the renderer.
#[derive(Clone, Copy, Debug)]
pub enum RowVertical {
    /// Span the full row height (selection background, line-highlight band).
    Full,
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
