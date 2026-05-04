//! Bridge: build a `DisplayLayout` from the legacy `(TextViewState, FoldState)` pair.
//!
//! This is intentionally a translation layer, not the real producer. The real
//! `display_map` transform stack (FoldMap → WrapMap → TabMap) gets wired in
//! step 9; this bridge exists so step 5 can run the new renderer in parallel
//! with the legacy one and assert equivalence.

use bevy::prelude::*;
use std::sync::Arc;

use crate::settings::{FontSettings, PerformanceSettings, SyntaxTheme};
use crate::text_view::layout::DisplayLayout;
use crate::text_view::snapshot::{ShapedLine, StyleRun};
use crate::text_view::state::TextViewState;
use crate::text_view::viewport::TextViewViewport;
use crate::types::{FoldState, LineSegment};

/// Build a `DisplayLayout` for the editor entity, with syntax highlighting
/// resolved inline per visible line.
///
/// The previous flow (legacy `update_gpu_text_instanced` → mutate
/// `TextViewState.styled_lines` → bridge reads it) has been collapsed: the
/// bridge now calls `syntax.highlight_range()` directly for the visible
/// window, eliminating the per-buffer-line `Vec<Option<Vec<LineSegment>>>`
/// materialization step. `TextViewState.styled_lines` is no longer read on
/// the rendering path; it'll be deleted in step 11.
#[allow(clippy::too_many_arguments)]
pub fn build_display_layout(
    state: &TextViewState,
    viewport: &TextViewViewport,
    fold_state: &FoldState,
    font: &FontSettings,
    performance: &PerformanceSettings,
    foreground_color: Color,
    syntax: Option<&mut crate::plugin::SyntaxResource>,
    syntax_theme: Option<&SyntaxTheme>,
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

    // Move the syntax + theme into a local Option so we can re-borrow per line.
    let mut syntax_opt = syntax;

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

        // Resolve styling for this line: syntax-highlight inline if available,
        // otherwise fall back to plain (empty runs → renderer uses default_fg).
        let segs: Vec<LineSegment> = match (syntax_opt.as_deref_mut(), syntax_theme) {
            (Some(syntax), Some(theme)) => {
                let mut hl = syntax.highlight_range(
                    &line_text,
                    buffer_line,
                    buffer_line + 1,
                    state.rope.line_to_byte(buffer_line),
                    theme,
                    foreground_color,
                );
                hl.pop().unwrap_or_default()
            }
            _ => Vec::new(),
        };
        let line_bg = segs.iter().find_map(|s| s.background);

        let mut runs: Vec<StyleRun> = Vec::with_capacity(segs.len());
        let mut byte_cursor = 0usize;
        for seg in &segs {
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
        // concatenation of segment texts (matches the byte_range indexing).
        // When runs is empty, fall back to the raw rope line.
        let render_text = if !runs.is_empty() {
            let mut s = String::with_capacity(byte_cursor);
            for seg in &segs {
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
            // both derive from this single anchor:
            //   glyph baseline = y_top + line_height/2 + baseline_offset
            //   overlay full-line rect spans y_top..y_top+line_height
            // The legacy code anchored to row *center* at
            // `text_area_top + scroll + row*line_height`; subtracting line_height/2
            // converts that to row top.
            y_top: viewport.text_area_top
                + state.scroll_offset
                + current_display_row as f32 * line_height
                - line_height * 0.5,
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
