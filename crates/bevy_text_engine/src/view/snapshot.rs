//! Snapshot types — what to render, owned by `text_view/`.
//!
//! These types are the renderer's data contract: a consumer (editor, chat panel,
//! log viewer) hands us styled lines + display layout; we render them. The renderer
//! does not know where the styling came from (syntax, markdown, plain text).

use bevy::prelude::*;
use std::ops::Range;
use std::sync::Arc;

/// One glyph from cosmic-text shaping. Rendered by looking up `cache_key` in the atlas.
///
/// `byte_index` is the cluster-start byte in the parent `ShapedLine.text` — a single
/// glyph may cover multiple bytes (ligatures, combining marks). Renderer consumers
/// that need a per-glyph color resolve it by binary-searching `ShapedLine.runs` on
/// `byte_index`.
#[derive(Clone, Copy, Debug)]
pub struct ShapedGlyph {
    /// Pen-x at glyph start, line-local in pixels (does not include `ShapedLine.x_offset`).
    pub x: f32,
    /// First byte in `ShapedLine.text` covered by this glyph.
    pub byte_index: usize,
    /// Atlas key — pass to `GlyphAtlas::get_or_rasterize_glyph`.
    pub cache_key: cosmic_text::CacheKey,
}

/// Per-line cosmic-text shaping result. Held by `ShapedLine.shape` as `Arc<LineShape>`
/// so scroll-only frames can reuse the previous frame's shape via `Arc::ptr_eq`.
#[derive(Clone, Debug)]
pub struct LineShape {
    /// Shaped glyphs in visual order. Indices align 1:1 with the cosmic-text
    /// `LayoutLine.glyphs` they were derived from.
    pub glyphs: Vec<ShapedGlyph>,
    /// Total advance of the line in pixels — equals last glyph's pen-x + last advance.
    /// Consumed by the display-map producer to drive `TextViewState.max_content_width`
    /// (the horizontal scrollbar's content extent).
    pub width: f32,
    /// Font size at which shaping was performed. Renderer compares against its own
    /// font_size and falls back to the char_width path on mismatch.
    pub font_size: f32,
}

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
    /// Byte offset within the buffer line where this row's `text` begins.
    /// Always 0 for non-wrapped rows; for soft-wrap continuations it's the
    /// byte index in the source line at which this row picks up. Lets
    /// consumers convert `(buffer_byte) → (display_row, byte_in_row)` without
    /// re-deriving from row text lengths.
    pub buffer_byte_offset: usize,
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
    /// Vertical space in pixels above this row, on top of the row's line
    /// height. Used for block-level spacing — heading top margins, paragraph
    /// breaks, code-block separators. Producers that stack rows must include
    /// this when computing `y_top`. Default 0.
    pub padding_top: f32,
    /// Vertical space in pixels below this row. See `padding_top`.
    pub padding_bottom: f32,
    /// Inline non-text content (images, spacers) anchored at byte offsets in
    /// `text`. Empty for plain-text consumers.
    pub inline_objects: Vec<InlineObject>,
    /// Per-glyph advances from cosmic-text shaping. `None` = use the layout's
    /// `char_width` fallback (cheap path for `trivial_layout` consumers like
    /// chat/log demos that don't want to pay shaping cost).
    pub shape: Option<Arc<LineShape>>,
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
            buffer_byte_offset: 0,
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
            padding_top: 0.0,
            padding_bottom: 0.0,
            inline_objects: Vec::new(),
            shape: None,
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

/// One row of input for [`trivial_layout_blocks`]: text + styling +
/// optional per-row line-height override + block padding above/below.
///
/// Markdown / chat consumers build a `Vec<TrivialBlock>` with appropriate
/// padding for paragraph breaks, heading margins, code-block separators,
/// and call [`trivial_layout_blocks`] to get a `DisplayLayout` with
/// correctly-stacked `y_top` values.
#[derive(Clone, Debug, Default)]
pub struct TrivialBlock {
    pub text: String,
    pub runs: Vec<StyleRun>,
    /// Per-row line-height in pixels. `None` = layout default.
    pub line_height: Option<f32>,
    pub padding_top: f32,
    pub padding_bottom: f32,
    pub line_bg: Option<Color>,
}

impl TrivialBlock {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Default::default()
        }
    }

    pub fn with_runs(mut self, runs: Vec<StyleRun>) -> Self {
        self.runs = runs;
        self
    }

    pub fn with_line_height(mut self, lh: f32) -> Self {
        self.line_height = Some(lh);
        self
    }

    pub fn with_padding(mut self, top: f32, bottom: f32) -> Self {
        self.padding_top = top;
        self.padding_bottom = bottom;
        self
    }

    pub fn with_background(mut self, color: Color) -> Self {
        self.line_bg = Some(color);
        self
    }
}

/// Like [`trivial_layout`] but accepts per-row line-height + padding.
///
/// Stacks rows by accumulating `padding_top + line_height + padding_bottom`
/// — markdown-style block layout. Rows where `line_height` is `None`
/// fall back to the layout's global `line_height`.
pub fn trivial_layout_blocks(
    blocks: &[TrivialBlock],
    line_height: f32,
    char_width: f32,
    baseline_offset: f32,
    default_fg: bevy::prelude::Color,
) -> super::layout::DisplayLayout {
    use super::layout::DisplayLayout;
    use std::sync::Arc;

    let mut shaped: Vec<ShapedLine> = Vec::with_capacity(blocks.len());
    let mut y = 0.0_f32;
    for (i, b) in blocks.iter().enumerate() {
        let row_h = b.line_height.unwrap_or(line_height);
        y += b.padding_top;
        shaped.push(ShapedLine {
            display_row: i as u32,
            buffer_row: i as u32,
            buffer_byte_offset: 0,
            is_wrap_continuation: false,
            y_top: y,
            x_offset: 0.0,
            text: b.text.clone(),
            runs: b.runs.clone(),
            line_bg: b.line_bg,
            line_height: b.line_height,
            padding_top: b.padding_top,
            padding_bottom: b.padding_bottom,
            inline_objects: Vec::new(),
            shape: None,
        });
        y += row_h + b.padding_bottom;
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Color;

    /// Block layout with mixed padding + per-row line-height stacks rows
    /// correctly: each row's `y_top` equals the sum of all prior rows'
    /// `(padding_top + line_height + padding_bottom) + this row's padding_top`.
    #[test]
    fn trivial_layout_blocks_stacks_padding_and_line_height() {
        let blocks = vec![
            // body: 16px line height, no padding.
            TrivialBlock::new("hello"),
            // heading: 24px line height, 8px above, 4px below.
            TrivialBlock::new("# heading")
                .with_line_height(24.0)
                .with_padding(8.0, 4.0),
            // body again, default line-height (16px), no padding.
            TrivialBlock::new("more body"),
            // code block row: 16px, 6px above, 6px below for the panel.
            TrivialBlock::new("fn x() {}").with_padding(6.0, 6.0),
        ];

        let layout = trivial_layout_blocks(&blocks, 16.0, 8.0, 5.0, Color::WHITE);
        let lines = &*layout.lines;
        assert_eq!(lines.len(), 4);

        // Row 0: y = 0
        assert_eq!(lines[0].y_top, 0.0);
        // Row 1: y = 0 + 16 (row0 lh) + 0 (row0 pad-bot) + 8 (row1 pad-top) = 24
        assert_eq!(lines[1].y_top, 24.0);
        // Row 2: y = 24 + 24 (row1 lh) + 4 (row1 pad-bot) + 0 = 52
        assert_eq!(lines[2].y_top, 52.0);
        // Row 3: y = 52 + 16 + 0 + 6 = 74
        assert_eq!(lines[3].y_top, 74.0);
    }

    #[test]
    fn trivial_layout_no_padding_matches_old_behavior() {
        let blocks: Vec<TrivialBlock> = ["a", "b", "c"]
            .iter()
            .map(|s| TrivialBlock::new(*s))
            .collect();
        let layout = trivial_layout_blocks(&blocks, 20.0, 8.0, 5.0, Color::WHITE);
        let lines = &*layout.lines;
        assert_eq!(lines[0].y_top, 0.0);
        assert_eq!(lines[1].y_top, 20.0);
        assert_eq!(lines[2].y_top, 40.0);
    }
}
