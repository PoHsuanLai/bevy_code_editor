//! Bridge: build a `DisplayLayout` from the legacy `(TextViewState, FoldState)` pair.
//!
//! This is intentionally a translation layer, not the real producer. The real
//! `display_map` transform stack (FoldMap → WrapMap → TabMap) gets wired in
//! step 9; this bridge exists so step 5 can run the new renderer in parallel
//! with the legacy one and assert equivalence.

use bevy::prelude::*;
use std::sync::Arc;

use crate::settings::{FontSettings, PerformanceSettings};
use crate::text_view::layout::DisplayLayout;
use crate::text_view::snapshot::{ShapedLine, StyleRun};
use crate::text_view::state::TextViewState;
use crate::text_view::viewport::TextViewViewport;
use crate::types::{FoldState, LineSegment};

/// Build a `DisplayLayout` from the current legacy data shape.
///
/// Mirrors the visible-range and fold-skipping logic from `render_text_view`
/// so that running `render_layout` against the result produces an equivalent
/// instance buffer.
pub fn build_display_layout(
    state: &TextViewState,
    viewport: &TextViewViewport,
    fold_state: &FoldState,
    font: &FontSettings,
    performance: &PerformanceSettings,
    foreground_color: Color,
) -> DisplayLayout {
    let line_height = font.line_height;
    let char_width = font.char_width;
    let baseline_offset = font.size * 0.32;
    let total_buffer_lines = state.line_count();

    // Visible range — same math as render_text_view.
    let buffer = line_height * performance.viewport_buffer_lines as f32;
    let scroll_dist = state.scroll_offset.abs();
    let start_pixels = scroll_dist - viewport.text_area_top - buffer;
    let first_visible_display_row = (start_pixels / line_height).floor().max(0.0) as u32;
    let visible_count =
        ((viewport.height as f32 + buffer * 2.0) / line_height).ceil() as u32;
    let last_visible_display_row = first_visible_display_row + visible_count;

    // Walk buffer lines, skipping folded ones, until we hit the first visible display row.
    let has_folding = !fold_state.regions.is_empty();
    let (start_buffer_line, mut current_display_row) = if has_folding {
        let mut display_row: u32 = 0;
        let mut buffer_line: usize = 0;
        while buffer_line < total_buffer_lines && display_row < first_visible_display_row {
            if !fold_state.is_line_hidden(buffer_line) {
                display_row += 1;
            }
            buffer_line += 1;
        }
        (buffer_line, display_row)
    } else {
        let start = (first_visible_display_row as usize).min(total_buffer_lines);
        (start, first_visible_display_row)
    };

    let mut shaped_lines: Vec<ShapedLine> = Vec::with_capacity(visible_count as usize);
    let visible_rows_start = current_display_row;

    for buffer_line in start_buffer_line..total_buffer_lines {
        if fold_state.is_line_hidden(buffer_line) {
            continue;
        }
        if current_display_row > last_visible_display_row {
            break;
        }

        let rope_line = state.rope.line(buffer_line);
        let line_text: String = rope_line.to_string();
        let line_x_extra = state
            .line_x_offsets
            .get(buffer_line)
            .copied()
            .unwrap_or(0.0);

        // Translate Vec<LineSegment> (positional) to Vec<StyleRun> (byte-range).
        // Empty -> empty runs (renderer falls back to default fg).
        let segs: &[LineSegment] = state
            .styled_lines
            .get(buffer_line)
            .and_then(|opt| opt.as_deref())
            .unwrap_or(&[]);
        let line_bg = segs.iter().find_map(|s| s.background);

        let mut runs: Vec<StyleRun> = Vec::with_capacity(segs.len());
        let mut byte_cursor = 0usize;
        for seg in segs {
            let len = seg.text.len();
            if len == 0 {
                continue;
            }
            runs.push(StyleRun {
                byte_range: byte_cursor..byte_cursor + len,
                fg: seg.color,
                bg: seg.background,
                font_scale: seg.font_scale,
                skew: seg.skew,
                corner_radius: seg.corner_radius,
            });
            byte_cursor += len;
        }

        // The text the renderer walks. When runs is non-empty, prefer the
        // concatenation of run texts (identical to legacy behavior, which
        // iterated `segment.text.chars()`). When runs is empty, use the rope line.
        let render_text = if !runs.is_empty() {
            let mut s = String::with_capacity(byte_cursor);
            for seg in segs {
                s.push_str(&seg.text);
            }
            s
        } else {
            line_text.clone()
        };

        shaped_lines.push(ShapedLine {
            display_row: current_display_row,
            buffer_row: buffer_line as u32,
            is_wrap_continuation: false,
            // y_top is the row's visual top in screen pixels. Glyphs and overlays
            // both derive from this single anchor: glyph baseline = y_top + ascent
            // (~ line_height/2 + baseline_offset); overlay full-line rect spans
            // y_top..y_top+line_height. The legacy "row center" convention is gone.
            y_top: viewport.text_area_top
                + state.scroll_offset
                + current_display_row as f32 * line_height,
            x_offset: line_x_extra,
            text: render_text,
            runs,
            line_bg,
        });

        current_display_row += 1;
    }

    let visible_rows_end = current_display_row;

    // Total display rows = total_buffer_lines minus folded ones (cheap iteration).
    let total_display_rows = if has_folding {
        (0..total_buffer_lines)
            .filter(|&l| !fold_state.is_line_hidden(l))
            .count() as u32
    } else {
        total_buffer_lines as u32
    };

    DisplayLayout {
        lines: Arc::new(shaped_lines),
        visible_rows: visible_rows_start..visible_rows_end,
        total_display_rows,
        line_height,
        char_width,
        baseline_offset,
        default_fg: foreground_color,
        version: 0,
        scroll_version: 0,
    }
}
