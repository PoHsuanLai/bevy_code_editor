//! Wrap-aware layout producer.
//!
//! Walks a visible window of `TextViewState.rope`, asks the caller per-line
//! whether the buffer line is hidden (folding hook) and what its styled runs
//! are (syntax/markdown hook), shapes each row through cosmic-text, and (when
//! soft wrap is enabled) splits long lines on a pixel-budget boundary into
//! multiple `ShapedLine` rows. The result is the per-frame `DisplayLayout`
//! consumed by the renderer and by cursor/selection/overlay producers.
//!
//! Editor-specific concepts (fold state, syntax provider, theme settings) are
//! injected via closures so the engine can serve markdown / chat / log-viewer
//! consumers directly without going through the editor.

use bevy::prelude::*;
use std::sync::Arc;

use super::font::FontConfig;
use super::layout::DisplayLayout;
use super::snapshot::{LineShape, ShapedGlyph, ShapedLine, StyleRun};
use super::state::TextViewState;
use super::viewport::TextViewViewport;
use crate::gpu::GlyphAtlas;

/// Inputs for [`build_display_layout`]. Plain primitives only — no editor
/// settings types — so any consumer (markdown, chat, log viewer) can build
/// these without depending on `bevy_code_editor`.
pub struct LayoutInputs<'a> {
    pub state: &'a mut TextViewState,
    pub viewport: &'a TextViewViewport,
    pub font: &'a FontConfig,
    pub atlas: Option<&'a mut GlyphAtlas>,
    pub fonts: Option<&'a Assets<bevy::text::Font>>,
    /// Pixel width budget for soft-wrap. `None` disables wrap and emits one
    /// `ShapedLine` per (visible) buffer line.
    pub wrap_budget_px: Option<f32>,
    /// Continuation-row left inset in pixels.
    pub wrap_indent_px: f32,
    /// Foreground color used when a line's styled-run list is empty.
    pub default_fg: Color,
    /// Extra rows kept above and below the visible window.
    pub viewport_buffer_lines: u32,
}

/// Build a `DisplayLayout` for the visible viewport.
///
/// `line_visible`: returns false for buffer lines hidden by folds. Pass
/// `|_| true` for non-folding consumers.
///
/// `line_style`: returns the styled runs for a buffer line, given the line
/// index and its text. The byte ranges in returned runs index into the
/// concatenation of run texts (which is what gets shaped). Empty `Vec` means
/// "render plain — renderer falls back to `default_fg`."
///
/// Most consumers will pass `|_, _| Vec::new()`. The editor's adapter wires
/// `line_style` to its tree-sitter highlighter; markdown consumers wire it to
/// their inline-styling pass.
pub fn build_display_layout(
    inputs: LayoutInputs<'_>,
    line_visible: impl Fn(usize) -> bool,
    mut line_style: impl FnMut(usize, &str) -> Vec<RunWithText>,
) -> DisplayLayout {
    let LayoutInputs {
        state,
        viewport,
        font,
        atlas,
        fonts,
        wrap_budget_px,
        wrap_indent_px,
        default_fg,
        viewport_buffer_lines,
    } = inputs;

    let line_height = font.line_height;
    let char_width = font.char_width;
    let baseline_offset = font.size * 0.32;
    let total_buffer_lines = state.line_count();

    // Visible range — same math as render_text_view.
    let buffer = line_height * viewport_buffer_lines as f32;
    let scroll_dist = state.scroll_offset.abs();
    let start_pixels = scroll_dist - viewport.text_area_top - buffer;
    let first_visible_display_row = (start_pixels / line_height).floor().max(0.0) as u32;
    let visible_count =
        ((viewport.height as f32 + buffer * 2.0) / line_height).ceil() as u32;
    let last_visible_display_row = first_visible_display_row + visible_count;

    // Walk buffer lines, skipping hidden ones, until we hit the first visible
    // display row. With wrap on, each buffer line may consume multiple display
    // rows; we approximate the per-line row count by char-count division to
    // skip ahead without shaping every line off-screen.
    //
    // Walking is the safe path: it's correct under both wrap and folds. We
    // skip it (taking the O(1) display_row == buffer_line shortcut) only
    // when we can prove neither shifts the mapping, by probing
    // `line_visible` along the prefix. The probe is `O(first_visible_row)`
    // and short-circuits on the first hidden line, so on un-folded buffers
    // it's a tight loop.
    let approx_wrap_chars = wrap_budget_px.map(|px| (px / char_width).max(1.0) as usize);
    let fast_path_start = (first_visible_display_row as usize).min(total_buffer_lines);
    let folding_in_play = approx_wrap_chars.is_none()
        && (0..fast_path_start).any(|l| !line_visible(l));
    let (start_buffer_line, mut current_display_row) =
        if approx_wrap_chars.is_some() || folding_in_play {
            let mut display_row: u32 = 0;
            let mut buffer_line: usize = 0;
            while buffer_line < total_buffer_lines && display_row < first_visible_display_row {
                if line_visible(buffer_line) {
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
            (fast_path_start, first_visible_display_row)
        };

    let mut shaped_lines: Vec<ShapedLine> = Vec::with_capacity(visible_count as usize);
    let visible_rows_start = current_display_row;

    let mut atlas_opt = atlas;

    for buffer_line in start_buffer_line..total_buffer_lines {
        if !line_visible(buffer_line) {
            continue;
        }
        if current_display_row > last_visible_display_row {
            break;
        }

        let rope_line = state.rope.line(buffer_line);
        let line_text: String = rope_line.to_string();

        // Caller produces styled runs (with their text payloads) for this
        // line. Empty list → render plain.
        let styled = line_style(buffer_line, &line_text);
        let line_bg = styled.iter().find_map(|s| s.run.bg);

        let mut runs: Vec<StyleRun> = Vec::with_capacity(styled.len());
        let mut byte_cursor = 0usize;
        let mut concat = String::new();
        for r in &styled {
            let len = r.text.len();
            if len == 0 {
                continue;
            }
            concat.push_str(&r.text);
            let mut run = r.run.clone();
            run.byte_range = byte_cursor..byte_cursor + len;
            runs.push(run);
            byte_cursor += len;
        }

        // The text the renderer walks. When runs is non-empty, prefer the
        // concatenation of run texts (matches the byte_range indexing).
        // When runs is empty, fall back to the raw rope line.
        let render_text = if !runs.is_empty() {
            concat
        } else {
            line_text.clone()
        };

        // Shape via cosmic-text when an atlas is available. Strip a trailing
        // newline first — the rope line includes it, but cosmic-text would
        // just emit a zero-advance glyph for it.
        let shape = atlas_opt.as_deref_mut().map(|atlas| {
            let shape_text = render_text.strip_suffix('\n').unwrap_or(&render_text);
            let font_id = match (font.font.as_ref(), fonts) {
                (Some(h), Some(fs)) => atlas.ensure_font(h, fs),
                _ => None,
            };
            Arc::new(atlas.shape_line(shape_text, font.size, font_id))
        });

        // Discover horizontal-scrollbar extent: track the widest shaped line
        // seen so far. Producer-driven so the consumer reads real pixel
        // widths.
        if let Some(s) = shape.as_ref() {
            if s.width > state.max_content_width {
                state.max_content_width = s.width;
                state.max_width_line = Some(buffer_line);
            }
        }

        // y_top for a given display_row.
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
                        padding_top: 0.0,
                        padding_bottom: 0.0,
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
                    padding_top: 0.0,
                    padding_bottom: 0.0,
                    inline_objects: Vec::new(),
                    shape,
                });
                current_display_row += 1;
            }
        }
    }

    let visible_rows_end = current_display_row;

    // Total display rows = sum over visible buffer lines of their wrap-row
    // count. With wrap off, that's just the visible buffer-line count.
    let total_display_rows: u32 = (0..total_buffer_lines)
        .filter(|&l| line_visible(l))
        .map(|l| approx_display_rows_for_line(&state.rope, l, approx_wrap_chars))
        .sum();

    DisplayLayout {
        lines: Arc::new(shaped_lines),
        visible_rows: visible_rows_start..visible_rows_end,
        total_display_rows,
        line_height,
        char_width,
        baseline_offset,
        default_fg,
        version: 0,
        scroll_version: 0,
    }
}

/// One styled run plus its text payload, returned by the `line_style` closure
/// in [`build_display_layout`]. The producer concatenates `text` payloads to
/// form the line that gets shaped, then rebases each run's `byte_range` to
/// match.
///
/// `run.byte_range` on input is ignored — the producer overwrites it with the
/// correct range based on the position of `text` in the concatenation. Set it
/// to `0..0` (or anything) when constructing.
#[derive(Clone, Debug)]
pub struct RunWithText {
    pub text: String,
    pub run: StyleRun,
}

/// One soft-wrap row's worth of post-shape data, ready to be packaged into a
/// `ShapedLine`. `glyphs` are line-local: each `g.x` has been rebased so the
/// row's first glyph starts at x=0.
#[derive(Clone, Debug)]
pub struct WrapRow {
    pub text: String,
    pub runs: Vec<StyleRun>,
    pub glyphs: Vec<ShapedGlyph>,
    pub width: f32,
    /// Byte offset within the source buffer line where this row's `text` starts.
    pub buffer_byte_offset: usize,
}

/// Split a shaped line into pixel-budgeted rows, preferring word-break
/// boundaries. The input `shape.glyphs[*].byte_index` are byte offsets into
/// `text`; emitted rows carry sliced text/runs and per-row local glyph x.
pub fn wrap_into_rows(
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
        // First glyph whose left edge exceeds the budget.
        let mut break_idx = shape.glyphs.len();
        for j in row_start_idx + 1..shape.glyphs.len() {
            let local_x = shape.glyphs[j].x - row_origin_x;
            if local_x > budget {
                break_idx = j;
                break;
            }
        }

        if break_idx == shape.glyphs.len() {
            // Remaining glyphs fit — final row.
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

        // Try to break at the previous space/tab cluster.
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
pub fn slice_runs(runs: &[StyleRun], range: std::ops::Range<usize>) -> Vec<StyleRun> {
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

/// Cheap approximate display-row count for a buffer line. Used for
/// off-screen row accounting (scrollbar sizing, scroll-offset →
/// first-visible-row translation) without paying the cost of full shaping.
pub fn approx_display_rows_for_line(
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
