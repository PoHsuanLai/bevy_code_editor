//! Editor adapter over the engine's wrap-aware layout producer.
//!
//! Turns editor-domain inputs (fold state, syntax highlighter, theme +
//! wrapping/indentation settings) into the plain-primitive `LayoutInputs`
//! the engine expects, captures syntax + fold lookups in closures, and
//! defers to [`bevy_text_engine::build_display_layout`].
//!
//! Behavior is byte-identical to the previous in-crate producer; only the
//! plumbing moved engine-side.

use bevy::prelude::*;
use bevy_text_engine::{
    build_display_layout as engine_build_display_layout, DisplayLayout, FontConfig, GlyphAtlas,
    LayoutInputs, RunWithText, StyleRun, TextViewState, TextViewViewport,
};
use std::cell::RefCell;

use crate::settings::{
    IndentationSettings, PerformanceSettings, SyntaxTheme, WrappingSettings,
};
use crate::types::{FoldState, LineSegment};

/// Build a `DisplayLayout` for the editor entity.
///
/// Resolves the wrap budget + indent from `wrapping`/`indentation`, captures
/// fold lookups and (optional) syntax highlighting in closures, and invokes
/// the engine's `build_display_layout`.
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
    fonts: Option<&Assets<bevy::text::Font>>,
) -> DisplayLayout {
    let char_width = font.char_width;

    // Wrap configuration. `wrap_budget_px` is the pixel width allotted to
    // text before a soft break. `None` means "no wrap" — emit one row per
    // buffer line.
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

    // Indent applied to soft-wrap continuation rows. Pixels, not chars.
    let wrap_indent_px = if wrapping.enabled && wrapping.indent_wrapped_lines {
        indentation.indent_size as f32 * char_width
    } else {
        0.0
    };

    // The `line_style` closure needs each line's start byte to feed
    // `SyntaxResource::highlight_range`, but we're about to hand `&mut state`
    // to the engine — we can't simultaneously borrow `&state.rope` from a
    // closure. Pre-compute the buffer-line → start-byte mapping. `Rope`'s
    // `line_to_byte` is `O(log n)` per call, so this is dominated by the
    // per-line shaping cost the engine pays anyway.
    let line_byte_offsets: Vec<usize> = (0..state.line_count())
        .map(|i| state.rope.line_to_byte(i))
        .collect();

    // Wrap the syntax highlighter in `RefCell` so the `FnMut` closure can
    // re-borrow it per line. The borrow is short (one line at a time) and
    // never re-entered, so this never panics.
    let syntax_cell = syntax.map(RefCell::new);

    let line_visible = |buffer_line: usize| -> bool { !fold_state.is_line_hidden(buffer_line) };

    let line_style = |buffer_line: usize, line_text: &str| -> Vec<RunWithText> {
        let segs: Vec<LineSegment> = match (syntax_cell.as_ref(), syntax_theme) {
            (Some(cell), Some(theme)) => {
                let start_byte = line_byte_offsets
                    .get(buffer_line)
                    .copied()
                    .unwrap_or(0);
                let mut syntax = cell.borrow_mut();
                let mut hl = syntax.highlight_range(
                    line_text,
                    buffer_line,
                    buffer_line + 1,
                    start_byte,
                    theme,
                    foreground_color,
                );
                hl.pop().unwrap_or_default()
            }
            _ => Vec::new(),
        };
        segs_to_runs(&segs)
    };

    let inputs = LayoutInputs {
        state,
        viewport,
        font,
        atlas,
        fonts,
        wrap_budget_px,
        wrap_indent_px,
        default_fg: foreground_color,
        viewport_buffer_lines: performance.viewport_buffer_lines as u32,
    };

    engine_build_display_layout(inputs, line_visible, line_style)
}

/// Convert editor-domain `LineSegment`s (the syntax-highlighting output) into
/// the engine's `(text, StyleRun)` payloads. The `byte_range` of each run is
/// overwritten by the engine after concatenation, so we leave it as `0..0`.
fn segs_to_runs(segs: &[LineSegment]) -> Vec<RunWithText> {
    segs.iter()
        .filter(|s| !s.text.is_empty())
        .map(|s| RunWithText {
            text: s.text.clone(),
            run: StyleRun {
                byte_range: 0..0,
                fg: s.color,
                bg: s.background,
                font_scale: s.font_scale,
                skew: s.skew,
                corner_radius: s.corner_radius,
                font_weight: None,
                font_family: None,
                decoration: None,
                link: None,
            },
        })
        .collect()
}
