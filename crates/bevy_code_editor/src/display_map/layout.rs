//! Bridge: build a `DisplayLayout` from the legacy `(TextViewState, FoldState)` pair.
//!
//! This is intentionally a translation layer, not the real producer. The real
//! `display_map` transform stack (FoldMap → WrapMap → TabMap) gets wired in
//! step 9; this bridge exists so step 5 can run the new renderer in parallel
//! with the legacy one and assert equivalence.

use bevy::prelude::*;
use bevy_text_engine::{FontConfig, GlyphAtlas};
use std::sync::Arc;

use crate::settings::{
    IndentationSettings, PerformanceSettings, SyntaxTheme, WrappingSettings,
};
use crate::text_view::layout::DisplayLayout;
use crate::text_view::snapshot::{ShapedLine, StyleRun};
use bevy_text_engine::view::snapshot::{LineShape, ShapedGlyph};
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
    state: &mut TextViewState,
    viewport: &TextViewViewport,
    fold_state: &FoldState,
    font: &FontConfig,
    performance: &PerformanceSettings,
    wrapping: &WrappingSettings,
    indentation: &IndentationSettings,
    foreground_color: Color,
    syntax: Option<&mut crate::plugin::SyntaxResource>,
    syntax_theme: Option<&SyntaxTheme>,
    atlas: Option<&mut GlyphAtlas>,
) -> DisplayLayout {
    let line_height = font.line_height;
    let char_width = font.char_width;
    let baseline_offset = font.size * 0.32;
    let total_buffer_lines = state.line_count();

    // Wrap configuration. `wrap_budget_px` is the pixel width allotted to text
    // before a soft break. `None` here means "no wrap" — emit one ShapedLine
    // per buffer line.
    let wrap_budget_px: Option<f32> = if wrapping.enabled {
        let viewport_text_w =
            (viewport.width as f32 - viewport.text_area_left).max(char_width);
        let budget = match wrapping.wrap_column {
            Some(col) => (col as f32) * char_width,
            None => viewport_text_w,
        };
        Some(budget.max(char_width))
    } else {
        None
    };

    // Indent applied to soft-wrap continuation rows. Matches the legacy
    // `indent_wrapped_lines` setting; pixels not chars.
    let wrap_indent_px = if wrapping.enabled && wrapping.indent_wrapped_lines {
        indentation.indent_size as f32 * char_width
    } else {
        0.0
    };

    // Visible range — same math as render_text_view.
    let buffer = line_height * performance.viewport_buffer_lines as f32;
    let scroll_dist = state.scroll_offset.abs();
    let start_pixels = scroll_dist - viewport.text_area_top - buffer;
    let first_visible_display_row = (start_pixels / line_height).floor().max(0.0) as u32;
    let visible_count =
        ((viewport.height as f32 + buffer * 2.0) / line_height).ceil() as u32;
    let last_visible_display_row = first_visible_display_row + visible_count;

    // Walk buffer lines, skipping folded ones, until we hit the first visible
    // display row. With wrap on, each buffer line may consume multiple display
    // rows; we approximate the per-line row count by char-count division to
    // skip ahead without shaping every line off-screen.
    let has_folding = !fold_state.regions.is_empty();
    let approx_wrap_chars = wrap_budget_px.map(|px| (px / char_width).max(1.0) as usize);
    let (start_buffer_line, mut current_display_row) = if has_folding || approx_wrap_chars.is_some()
    {
        let mut display_row: u32 = 0;
        let mut buffer_line: usize = 0;
        while buffer_line < total_buffer_lines && display_row < first_visible_display_row {
            if !fold_state.is_line_hidden(buffer_line) {
                let rows = approx_display_rows_for_line(
                    &state.rope,
                    buffer_line,
                    approx_wrap_chars,
                );
                display_row += rows;
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
    let mut atlas_opt = atlas;

    for buffer_line in start_buffer_line..total_buffer_lines {
        if fold_state.is_line_hidden(buffer_line) {
            continue;
        }
        if current_display_row > last_visible_display_row {
            break;
        }

        let rope_line = state.rope.line(buffer_line);
        let line_text: String = rope_line.to_string();

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
                font_weight: None,
                font_family: None,
                decoration: None,
                link: None,
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

        // Shape via cosmic-text when an atlas is available. Strip a trailing
        // newline first — the rope line includes it, but cosmic-text would just
        // emit a zero-advance glyph for it.
        let shape = atlas_opt.as_deref_mut().map(|atlas| {
            let shape_text = render_text.strip_suffix('\n').unwrap_or(&render_text);
            Arc::new(atlas.shape_line(shape_text, font.size))
        });

        // Discover horizontal-scrollbar extent: track the widest shaped line
        // we've seen so far. Producer-driven so the consumer (h-scroll thumb in
        // `mouse.rs`/`ui_elements.rs`) reads real pixel widths.
        if let Some(s) = shape.as_ref() {
            if s.width > state.max_content_width {
                state.max_content_width = s.width;
                state.max_width_line = Some(buffer_line);
            }
        }

        // y_top for a given display_row, derived from the same anchor the
        // legacy code used: row *top* in screen pixels = scroll-offset baseline
        // + display_row * line_height - line_height/2 (legacy anchored to row
        // center; we anchor to top).
        let y_top_for = |display_row: u32| -> f32 {
            viewport.text_area_top
                + state.scroll_offset
                + display_row as f32 * line_height
                - line_height * 0.5
        };

        // When wrap is on and the shaped line exceeds the budget, split into
        // multiple rows. Otherwise emit a single row covering the full text.
        let wrap_split = match (wrap_budget_px, shape.as_ref()) {
            (Some(budget), Some(s)) if s.width > budget => {
                Some(wrap_into_rows(&render_text, &runs, s, budget))
            }
            _ => None,
        };

        match wrap_split {
            Some(rows) if !rows.is_empty() => {
                for (i, row) in rows.iter().enumerate() {
                    let row_shape = Arc::new(LineShape {
                        glyphs: row.glyphs.clone(),
                        width: row.width,
                        font_size: shape.as_ref().map(|s| s.font_size).unwrap_or(font.size),
                    });
                    shaped_lines.push(ShapedLine {
                        display_row: current_display_row,
                        buffer_row: buffer_line as u32,
                        buffer_byte_offset: row.buffer_byte_offset,
                        is_wrap_continuation: i > 0,
                        y_top: y_top_for(current_display_row),
                        x_offset: if i > 0 { wrap_indent_px } else { 0.0 },
                        text: row.text.clone(),
                        runs: row.runs.clone(),
                        line_bg,
                        line_height: None,
                        inline_objects: Vec::new(),
                        shape: Some(row_shape),
                    });
                    current_display_row += 1;
                }
            }
            _ => {
                shaped_lines.push(ShapedLine {
                    display_row: current_display_row,
                    buffer_row: buffer_line as u32,
                    buffer_byte_offset: 0,
                    is_wrap_continuation: false,
                    y_top: y_top_for(current_display_row),
                    x_offset: 0.0,
                    text: render_text,
                    runs,
                    line_bg,
                    line_height: None,
                    inline_objects: Vec::new(),
                    shape,
                });
                current_display_row += 1;
            }
        }
    }

    let visible_rows_end = current_display_row;

    // Total display rows = sum over unfolded buffer lines of their wrap-row
    // count. With wrap off, that's just the unfolded buffer-line count.
    let total_display_rows: u32 = (0..total_buffer_lines)
        .filter(|&l| !fold_state.is_line_hidden(l))
        .map(|l| approx_display_rows_for_line(&state.rope, l, approx_wrap_chars))
        .sum();

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

/// One soft-wrap row's worth of post-shape data, ready to be packaged into
/// a `ShapedLine`. `glyphs` are line-local: each `g.x` has been rebased so
/// the row's first glyph starts at x=0.
struct WrapRow {
    text: String,
    runs: Vec<StyleRun>,
    glyphs: Vec<ShapedGlyph>,
    width: f32,
    /// Byte offset within the source buffer line where this row's `text` starts.
    buffer_byte_offset: usize,
}

/// Split a shaped line into pixel-budgeted rows, preferring word-break
/// boundaries. The input `shape.glyphs[*].byte_index` are byte offsets into
/// `text`; emitted rows carry sliced text/runs and per-row local glyph x.
fn wrap_into_rows(
    text: &str,
    runs: &[StyleRun],
    shape: &LineShape,
    budget: f32,
) -> Vec<WrapRow> {
    if shape.glyphs.is_empty() || text.is_empty() {
        return Vec::new();
    }

    let mut rows: Vec<WrapRow> = Vec::new();
    let mut row_start_idx: usize = 0; // index into shape.glyphs
    let mut row_start_x: f32 = 0.0;

    while row_start_idx < shape.glyphs.len() {
        let row_origin_x = row_start_x;
        // Find the first glyph whose right edge (x + advance ≈ next glyph x)
        // exceeds the budget. We approximate the advance as `next_x - this_x`;
        // for the last glyph use shape.width as the right edge.
        let mut break_idx = shape.glyphs.len();
        for j in row_start_idx + 1..shape.glyphs.len() {
            let local_x = shape.glyphs[j].x - row_origin_x;
            if local_x > budget {
                break_idx = j;
                break;
            }
        }

        if break_idx == shape.glyphs.len() {
            // The remaining glyphs fit entirely in this row — final row.
            let row_glyphs: Vec<ShapedGlyph> = shape.glyphs[row_start_idx..]
                .iter()
                .map(|g| ShapedGlyph {
                    x: g.x - row_origin_x,
                    byte_index: g.byte_index - shape.glyphs[row_start_idx].byte_index,
                    cache_key: g.cache_key,
                })
                .collect();
            let buf_byte_start = shape.glyphs[row_start_idx].byte_index;
            let row_text = text[buf_byte_start..].to_string();
            let row_runs = slice_runs(runs, buf_byte_start..text.len());
            let row_width = shape.width - row_origin_x;
            rows.push(WrapRow {
                text: row_text,
                runs: row_runs,
                glyphs: row_glyphs,
                width: row_width,
                buffer_byte_offset: buf_byte_start,
            });
            break;
        }

        // Try to break at the previous space/tab cluster. Look back from
        // break_idx-1 for a glyph whose byte starts a whitespace char.
        let mut chosen = break_idx;
        for j in (row_start_idx + 1..break_idx).rev() {
            let g = &shape.glyphs[j];
            if let Some(ch) = text[g.byte_index..].chars().next() {
                if ch == ' ' || ch == '\t' {
                    chosen = j + 1; // break *after* the whitespace
                    break;
                }
            }
        }

        // Avoid infinite loop on a single oversized glyph.
        if chosen <= row_start_idx {
            chosen = (row_start_idx + 1).min(shape.glyphs.len());
        }

        let row_byte_end = if chosen < shape.glyphs.len() {
            shape.glyphs[chosen].byte_index
        } else {
            text.len()
        };
        let row_byte_start = shape.glyphs[row_start_idx].byte_index;
        let row_glyphs: Vec<ShapedGlyph> = shape.glyphs[row_start_idx..chosen]
            .iter()
            .map(|g| ShapedGlyph {
                x: g.x - row_origin_x,
                byte_index: g.byte_index - row_byte_start,
                cache_key: g.cache_key,
            })
            .collect();
        let row_text = text[row_byte_start..row_byte_end].to_string();
        let row_runs = slice_runs(runs, row_byte_start..row_byte_end);
        let row_width = if chosen < shape.glyphs.len() {
            shape.glyphs[chosen].x - row_origin_x
        } else {
            shape.width - row_origin_x
        };
        rows.push(WrapRow {
            text: row_text,
            runs: row_runs,
            glyphs: row_glyphs,
            width: row_width,
            buffer_byte_offset: row_byte_start,
        });

        row_start_idx = chosen;
        row_start_x = if chosen < shape.glyphs.len() {
            shape.glyphs[chosen].x
        } else {
            shape.width
        };
    }

    rows
}

/// Clip and rebase a slice of runs to a byte sub-range.
fn slice_runs(runs: &[StyleRun], range: std::ops::Range<usize>) -> Vec<StyleRun> {
    let mut out = Vec::new();
    for run in runs {
        if run.byte_range.end <= range.start || run.byte_range.start >= range.end {
            continue;
        }
        let s = run.byte_range.start.max(range.start) - range.start;
        let e = run.byte_range.end.min(range.end) - range.start;
        if s >= e {
            continue;
        }
        out.push(StyleRun {
            byte_range: s..e,
            fg: run.fg,
            bg: run.bg,
            font_scale: run.font_scale,
            skew: run.skew,
            corner_radius: run.corner_radius,
            font_weight: run.font_weight,
            font_family: run.font_family.clone(),
            decoration: run.decoration,
            link: run.link.clone(),
        });
    }
    out
}

/// Cheap approximate display-row count for a buffer line. Used for off-screen
/// row accounting (scrollbar sizing, scroll-offset → first-visible-row
/// translation) without paying the cost of full shaping.
fn approx_display_rows_for_line(
    rope: &ropey::Rope,
    buffer_line: usize,
    wrap_chars: Option<usize>,
) -> u32 {
    let Some(budget) = wrap_chars else {
        return 1;
    };
    if buffer_line >= rope.len_lines() {
        return 1;
    }
    let line = rope.line(buffer_line);
    let mut len = line.len_chars();
    if len > 0 && line.char(len - 1) == '\n' {
        len -= 1;
    }
    if len == 0 {
        1
    } else {
        len.div_ceil(budget) as u32
    }
}
