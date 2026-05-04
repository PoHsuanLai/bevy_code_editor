//! Snapshot types — what to render, owned by `text_view/`.
//!
//! These types are the renderer's data contract: a consumer (editor, chat panel,
//! log viewer) hands us styled lines + display layout; we render them. The renderer
//! does not know where the styling came from (syntax, markdown, plain text).

use bevy::prelude::*;
use std::ops::Range;
use std::sync::Arc;

/// Text decoration applied across a `StyleRun`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextDecoration {
    Underline,
    Strikethrough,
    /// Wavy underline (typically for diagnostics).
    Squiggle,
}

/// Inline non-text content placed at a byte offset within `ShapedLine.text`.
///
/// The byte offset references a position inside `text` (typically a zero-width
/// marker character or a placeholder). The renderer reserves horizontal space
/// matching the object's intrinsic size and skips glyph emission at that offset.
#[derive(Clone, Debug)]
pub enum InlineObject {
    /// An image anchored to a byte offset; `size` is the rendered pixel rect.
    Image {
        byte_offset: usize,
        handle: Handle<Image>,
        size: Vec2,
    },
    /// Reserved horizontal whitespace (no visual). Useful for tab-like indents
    /// that aren't expressible via the run's text.
    Spacer { byte_offset: usize, width: f32 },
}

/// A run of text within a shaped line that shares the same style.
///
/// Byte ranges index into the parent `ShapedLine.text` (post-fold/wrap), so
/// runs are sparse and the renderer doesn't materialize per-buffer-line
/// `Vec<Option<…>>` arrays.
///
/// Most fields are `Option`s with `None` meaning "use the layout default."
/// This keeps cheap monospace consumers untouched: an editor that only needs
/// foreground color and italic skew leaves weight / family / decoration / link
/// all `None` and pays nothing extra.
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
    /// Font weight (100..=900). `None` = layout default.
    ///
    /// Currently informational: the renderer ships a single font face, so
    /// distinct weights produce identical output. A future phase will load
    /// per-weight faces and key the glyph atlas by `(char, size, weight,
    /// family)`. Producers should still set this so the data contract is
    /// in place when the rasterizer catches up.
    pub font_weight: Option<u16>,
    /// Font family name. `None` = layout default. `Arc<str>` so consumers can
    /// reuse the same family pointer across many runs cheaply.
    ///
    /// Same caveat as `font_weight`: currently informational.
    pub font_family: Option<Arc<str>>,
    /// Decoration drawn alongside the text (underline/strikethrough/squiggle).
    /// Currently informational; rendering pass landing in a follow-up phase.
    pub decoration: Option<TextDecoration>,
    /// URL or anchor target if this run is a link. Click handlers in the
    /// interaction layer can dispatch on this.
    pub link: Option<Arc<str>>,
}

impl StyleRun {
    /// Convenience constructor: a run with foreground color only (everything
    /// else default). Mirrors the pre-Phase-4 `LineSegment` shape.
    pub fn fg_only(byte_range: Range<usize>, fg: Color) -> Self {
        Self {
            byte_range,
            fg,
            bg: None,
            font_scale: 0.0,
            skew: 0.0,
            corner_radius: 0.0,
            font_weight: None,
            font_family: None,
            decoration: None,
            link: None,
        }
    }
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
    /// Pre-computed Y position in pixels relative to the layout origin. The
    /// renderer trusts this value and does not recompute it from
    /// `display_row * line_height` — important when `line_height` overrides
    /// produce non-uniform row heights (markdown headings, code blocks).
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
    /// Per-row line-height override in pixels. `None` = use the layout's
    /// global `line_height`. Producers that emit non-uniform rows (markdown
    /// headings, code blocks at a different size) set this; helpers that
    /// stack rows must compute `y_top` accordingly (see `trivial_layout`).
    pub line_height: Option<f32>,
    /// Inline non-text content (images, spacers) anchored at byte offsets in
    /// `text`. Empty for plain-text consumers.
    pub inline_objects: Vec<InlineObject>,
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
            line_height: None,
            inline_objects: Vec::new(),
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
