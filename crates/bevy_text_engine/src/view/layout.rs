//! `DisplayLayout` — the immutable, paint-ready snapshot the renderer consumes.
//!
//! The Warp insight: a frame's render is a pure function of its layout. We get
//! cheap "nothing changed" by comparing `Arc::ptr_eq(&prev.lines, &next.lines)`
//! and skipping the GPU upload entirely. Scroll-only updates bump
//! `scroll_version` but reuse the same `lines` Arc.

use bevy::prelude::*;
use std::ops::Range;
use std::sync::Arc;

use super::snapshot::{BlockRect, ShapedLine};

/// Per-entity rendering snapshot. Replaces the dirty-flag dance on `TextViewState`.
///
/// Written by `display_map::build_display_layout` (or by `trivial_layout` for
/// standalone consumers). Read-only for the renderer.
// not reflectable: paint-time snapshot built every frame; contains
// `Arc<Vec<ShapedLine>>` whose leaf `cosmic_text::CacheKey` and `Arc<LineShape>`
// are not Reflect. The component is rebuilt every frame from the rope so
// inspector access would observe stale data anyway.
#[derive(Component, Clone)]
pub struct DisplayLayout {
    /// Visible-window slice of shaped lines. Shared (Arc) so scroll-only and
    /// content-only paths can swap one without rebuilding the other.
    pub lines: Arc<Vec<ShapedLine>>,
    /// Block-level decorations (background fills, borders) for static-content
    /// consumers (markdown / chat panels). Empty for editor-driven layouts —
    /// the editor uses `ShapedLine.line_bg` and per-run backgrounds. The
    /// renderer paints these as a layer below per-row backgrounds.
    pub block_rects: Arc<Vec<BlockRect>>,
    /// Display row range covered by `lines` (absolute, into the full document).
    pub visible_rows: Range<u32>,
    /// Total display row count for the entire document (for scrollbar sizing).
    pub total_display_rows: u32,
    pub line_height: f32,
    /// Width of one column in pixels. Monospace assumption — for proportional fonts
    /// this is a hint only and per-glyph advance from shaping wins.
    pub char_width: f32,
    /// Vertical baseline offset within a line, in pixels.
    pub baseline_offset: f32,
    /// Default foreground color when a `ShapedLine.runs` is empty.
    pub default_fg: Color,
    /// Bumps when content / wrap / fold / styling changes (anything that invalidates `lines`).
    pub version: u64,
    /// Bumps independently when only scroll changed — same `lines`, different viewport slice.
    pub scroll_version: u64,
}

impl Default for DisplayLayout {
    fn default() -> Self {
        Self {
            lines: Arc::new(Vec::new()),
            block_rects: Arc::new(Vec::new()),
            visible_rows: 0..0,
            total_display_rows: 0,
            line_height: 16.0,
            char_width: 8.0,
            baseline_offset: 0.0,
            default_fg: Color::WHITE,
            version: 0,
            scroll_version: 0,
        }
    }
}

impl DisplayLayout {
    /// True when the same `lines` Arc backs both layouts (cheap nothing-changed check).
    pub fn lines_unchanged(&self, other: &DisplayLayout) -> bool {
        Arc::ptr_eq(&self.lines, &other.lines)
    }

    /// Pixel x where `byte` begins within `display_row`, line-local (does not
    /// include `ShapedLine.x_offset`). Uses shaped advances when present;
    /// falls back to a `char_width` walk over `text` otherwise.
    ///
    /// Returns `None` if `display_row` is not in this layout's visible window.
    pub fn x_at_byte(&self, display_row: u32, byte: usize) -> Option<f32> {
        let line = self.lines.iter().find(|l| l.display_row == display_row)?;
        Some(line_x_at_byte(line, byte, self.char_width))
    }

    /// Byte offset within `display_row` at pixel x (line-local). Inverse of `x_at_byte`.
    /// Snaps to the nearest cluster boundary using shaped advances when present.
    ///
    /// Returns `None` if `display_row` is not in this layout's visible window.
    pub fn byte_at_x(&self, display_row: u32, x: f32) -> Option<usize> {
        let line = self.lines.iter().find(|l| l.display_row == display_row)?;
        Some(line_byte_at_x(line, x, self.char_width))
    }

    /// Map a `(buffer_row, byte_in_buffer_line)` position to the display row
    /// and the byte offset within that row's `text`. Walks all rows sharing
    /// the buffer line to find the one whose `[buffer_byte_offset .. + text.len()]`
    /// covers the byte. Returns `None` if no row in the visible window matches.
    pub fn buffer_to_display(&self, buffer_row: u32, byte_in_line: usize) -> Option<(u32, usize)> {
        // Rows are emitted in display order; rows sharing a buffer_row are in
        // ascending buffer_byte_offset. We pick the row with the largest
        // buffer_byte_offset that's <= byte_in_line.
        let mut best: Option<&ShapedLine> = None;
        for line in self.lines.iter() {
            if line.buffer_row != buffer_row {
                continue;
            }
            if line.buffer_byte_offset <= byte_in_line {
                best = Some(line);
            } else {
                break;
            }
        }
        let line = best?;
        let local = byte_in_line.saturating_sub(line.buffer_byte_offset);
        Some((line.display_row, local.min(line.text.len())))
    }
}

/// Line-local pixel x for a byte offset inside `line.text`. Public-in-crate so
/// `render.rs` can reuse it for run start positions and bg widths.
pub(crate) fn line_x_at_byte(
    line: &ShapedLine,
    byte: usize,
    char_width_fallback: f32,
) -> f32 {
    if let Some(shape) = &line.shape {
        // Visible lines are short — linear scan beats the binary-search overhead.
        // Cluster starts are monotonic for LTR; for BiDi runs the byte_index
        // ordering may not match visual order but the renderer doesn't paint
        // BiDi yet, so scanning is correct in practice.
        for g in &shape.glyphs {
            if g.byte_index >= byte {
                return g.x;
            }
        }
        return shape.width;
    }
    let prefix = line.text.get(..byte).unwrap_or("");
    let mut x = 0.0;
    for ch in prefix.chars() {
        if ch == '\t' {
            x += char_width_fallback * 4.0;
        } else if ch != '\n' && ch != '\r' {
            x += char_width_fallback;
        }
    }
    x
}

/// Inverse of [`line_x_at_byte`] — snap a line-local pixel x to the nearest
/// cluster boundary in `line.text`.
pub(crate) fn line_byte_at_x(
    line: &ShapedLine,
    x: f32,
    char_width_fallback: f32,
) -> usize {
    if let Some(shape) = &line.shape {
        if x <= 0.0 {
            return shape.glyphs.first().map(|g| g.byte_index).unwrap_or(0);
        }
        for window in shape.glyphs.windows(2) {
            let cur = &window[0];
            let next = &window[1];
            if x < next.x {
                let mid = (cur.x + next.x) * 0.5;
                return if x < mid { cur.byte_index } else { next.byte_index };
            }
        }
        return line.text.len();
    }
    if char_width_fallback <= 0.0 {
        return 0;
    }
    let col = (x / char_width_fallback).max(0.0) as usize;
    let mut byte = 0;
    let mut current_col = 0;
    for ch in line.text.chars() {
        if current_col >= col || ch == '\n' || ch == '\r' {
            break;
        }
        if ch == '\t' {
            current_col += 4;
        } else {
            current_col += 1;
        }
        byte += ch.len_utf8();
    }
    byte
}
