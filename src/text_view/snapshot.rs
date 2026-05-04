//! Snapshot types — what to render, owned by `text_view/`.
//!
//! These types are the renderer's data contract: a consumer (editor, chat panel,
//! log viewer) hands us styled lines + display layout; we render them. The renderer
//! does not know where the styling came from (syntax, markdown, plain text).

use bevy::prelude::*;
use std::ops::Range;

/// A run of text within a shaped line that shares the same style.
///
/// Replaces `LineSegment`. The key difference: `byte_range` is keyed against
/// the line's `text` field (post-fold/wrap), so runs are sparse and the renderer
/// doesn't have to materialize a `Vec<Option<…>>` per buffer line.
#[derive(Clone, Debug)]
pub struct StyleRun {
    /// Byte range within the parent `ShapedLine.text`. Sorted, non-overlapping.
    pub byte_range: Range<usize>,
    pub fg: Color,
    pub bg: Option<Color>,
    /// 1.0 = normal, 1.3 = header, etc. 0.0 means use line default.
    pub font_scale: f32,
    /// Horizontal skew for italic simulation (~0.2 = italic).
    pub skew: f32,
    pub corner_radius: f32,
}

/// One display row's worth of text + styling, ready to render.
///
/// Produced by `display_map::build_display_layout`. Folding, soft-wrap, and tab
/// expansion have already been applied — `text` is exactly what appears on screen
/// for this row, `runs` covers it.
#[derive(Clone, Debug)]
pub struct ShapedLine {
    /// Display row index (0-based, post-fold/wrap).
    pub display_row: u32,
    /// Source buffer line. Multiple display rows may share a buffer row when wrapped.
    pub buffer_row: u32,
    /// True when this row is a soft-wrap continuation of the previous row.
    pub is_wrap_continuation: bool,
    /// Pre-computed Y position in pixels (= display_row * line_height).
    pub y_top: f32,
    /// Per-line X offset (right-align, indent-of-wrap, etc.).
    pub x_offset: f32,
    /// The text to render for this row, post-fold/wrap/tab expansion.
    pub text: String,
    /// Styled runs covering `text`. Sorted by `byte_range.start`, non-overlapping.
    /// Empty = render as plain text using the layout's default foreground.
    pub runs: Vec<StyleRun>,
    /// Optional full-line background.
    pub line_bg: Option<Color>,
}

/// Trivial styling source: one foreground color for the whole text.
///
/// Used by standalone consumers (chat panel, log viewer) that don't need
/// syntax highlighting. The editor uses a real `HighlightMap` instead.
#[derive(Clone, Copy, Debug)]
pub struct SimpleTheme {
    pub foreground: Color,
    pub background: Option<Color>,
}

impl SimpleTheme {
    pub fn new(foreground: Color) -> Self {
        Self {
            foreground,
            background: None,
        }
    }
}

/// Build a `DisplayLayout` from plain text + per-line styling, suitable for
/// standalone consumers that don't have an editor / display map.
///
/// `lines` is a list of `(text, runs)` pairs — one entry per buffer line, in
/// order. `runs` is the styling for that line (empty = use `default_fg`).
/// No folding, no soft-wrap, no viewport culling — every line is included.
pub fn trivial_layout(
    lines: &[(String, Vec<StyleRun>)],
    line_height: f32,
    char_width: f32,
    baseline_offset: f32,
    default_fg: bevy::prelude::Color,
) -> super::layout::DisplayLayout {
    use super::layout::DisplayLayout;
    use std::sync::Arc;

    let shaped: Vec<ShapedLine> = lines
        .iter()
        .enumerate()
        .map(|(i, (text, runs))| ShapedLine {
            display_row: i as u32,
            buffer_row: i as u32,
            is_wrap_continuation: false,
            // y_top is the row's visual top in screen-Y. Caller's render system
            // adds the viewport's text_area_top + scroll_offset on top if needed;
            // for a static demo we just stack rows from y=0.
            y_top: i as f32 * line_height,
            x_offset: 0.0,
            text: text.clone(),
            runs: runs.clone(),
            line_bg: None,
        })
        .collect();
    let total = shaped.len() as u32;
    DisplayLayout {
        lines: Arc::new(shaped),
        visible_rows: 0..total,
        total_display_rows: total,
        line_height,
        char_width,
        baseline_offset,
        default_fg,
        version: 1,
        scroll_version: 0,
    }
}
