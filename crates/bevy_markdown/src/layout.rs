//! Markdown Block IR → engine `Block` stream.
//!
//! Walks `Vec<parse::Block>` and emits a flat `Vec<bevy_instanced_text::Block>`
//! that the engine's `Block::layout` stacks into a `DisplayLayout`. Each
//! engine block carries its own `StyleRun`s for per-character styling
//! (bold/italic/inline code/links) and may bring `block_bg` /
//! `block_border` decorations for code blocks and blockquotes.
//!
//! Soft-wrap is delegated to the engine via `default_wrap_chars`.

use std::ops::Range;
use std::sync::Arc;

use bevy::text::Font;
use bevy::asset::Handle;
use bevy_instanced_text::{
    for_each_row_in_buffer_span, Block as EBlock, BlockLayoutConfig, DisplayLayout, RectOverlay,
    RowPosition, RowVertical, StyleRun, TextDecoration,
};

use crate::parse::{Block, Inline};
use crate::theme::MarkdownTheme;
use crate::view::{LinkSpan, MarkdownLinks};

/// Z order for code-block / blockquote backgrounds. Below text (negative)
/// and below selection too — selection should highlight on top of the
/// code-block bg, not be hidden by it.
const DECORATION_Z: i8 = -2;
/// Z for the blockquote left bar — sits above the body bg but still
/// below text.
const BLOCKQUOTE_BAR_Z: i8 = -1;

/// Configuration for one markdown layout pass.
#[derive(Clone, Debug)]
pub struct MarkdownLayoutConfig {
    pub theme: MarkdownTheme,
    pub line_height: f32,
    pub char_width: f32,
    pub baseline_offset: f32,
    /// Soft-wrap budget in characters. `None` ⇒ no wrap.
    pub wrap_chars: Option<usize>,
    /// Font used for inline code and fenced code blocks. `None` = same as body font.
    pub code_font: Option<Handle<Font>>,
}

impl MarkdownLayoutConfig {
    /// Build from font metrics with a pixel wrap budget. The budget is
    /// converted to characters via `char_width`; clamped to ≥8 so a
    /// pathologically narrow viewport doesn't produce zero-width wraps.
    pub fn from_metrics(
        theme: MarkdownTheme,
        line_height: f32,
        char_width: f32,
        baseline_offset: f32,
        wrap_width_px: Option<f32>,
    ) -> Self {
        let wrap_chars = wrap_width_px.map(|w| ((w / char_width).floor() as usize).max(8));
        Self {
            theme,
            line_height,
            char_width,
            baseline_offset,
            wrap_chars,
            code_font: None,
        }
    }
}

/// Produce a `DisplayLayout`, the row-anchored decoration overlays,
/// and the `MarkdownLinks` side-channel from parsed markdown blocks.
///
/// Decorations (code-block backgrounds, blockquote bars, horizontal
/// rules) come back as a `Vec<RectOverlay>` — anchored to display rows
/// rather than baked into block_rects, so a scroll-shifted layout
/// places them correctly without re-emission.
pub fn layout_markdown(
    blocks: &[Block],
    cfg: &MarkdownLayoutConfig,
) -> (DisplayLayout, Vec<RectOverlay>, MarkdownLinks) {
    let mut builder = LayoutBuilder::new(cfg);
    builder.emit_blocks(blocks, 0);
    builder.finish()
}

/// Builds the engine block list while collecting link byte ranges.
struct LayoutBuilder<'a> {
    cfg: &'a MarkdownLayoutConfig,
    blocks: Vec<EBlock>,
    pending_links: Vec<PendingLink>,
    /// Pending row-overlay records — resolved to `RectOverlay`s in
    /// `finish()` once display rows are known. Each entry references
    /// `block_idx` (index into `self.blocks`) so we can look up the
    /// run of display rows that block produced after `Block::layout`.
    pending_rects: Vec<PendingRect>,
}

/// One row-overlay that needs to be resolved against the laid-out
/// `DisplayLayout`. `block_idx` is the engine-block index whose rows
/// this overlay should cover; `coverage` says whether to include just
/// that block or extend through a contiguous span starting at it.
///
/// `builder` receives the row's `RowPosition` within the overlay's
/// span — it can use that to round only the top corners of the first
/// row and only the bottom corners of the last row, leaving middle
/// rows sharp so a multi-row panel reads as one rounded rectangle.
/// `aux` carries overlay-specific scalar data (e.g. outer indent for
/// blockquote bars); unused builders receive `0.0`.
struct PendingRect {
    block_idx: usize,
    coverage: RectCoverage,
    builder: fn(u32, RowPosition, &OverlayContext, f32) -> RectOverlay,
    /// Overlay-specific auxiliary value passed verbatim to `builder`.
    aux: f32,
}

/// Context passed to overlay builders at resolution time. Carries the
/// layout's wrap budget so an overlay can clip to the content area
/// rather than stretch to the viewport edge.
#[derive(Clone, Copy)]
struct OverlayContext<'a> {
    theme: &'a MarkdownTheme,
    /// Right edge of the content area in line-local pixels. Same number
    /// as the soft-wrap budget — tracks `viewport.width` minus margins.
    content_right: f32,
}

#[derive(Clone, Copy)]
enum RectCoverage {
    /// All rows belonging to a single engine block.
    SingleBlock,
    /// All rows from `block_idx` through the engine block at `block_idx + len - 1`,
    /// inclusive. Used for code blocks (one engine block per source line)
    /// and blockquote bars (spans multiple engine blocks).
    Span { len: usize },
}

/// A link range recorded against the engine block carrying its text.
/// Resolved to `(display_row, byte_range)` after layout finishes.
struct PendingLink {
    block_idx: usize,
    byte_range: Range<usize>,
    url: Arc<str>,
}

impl<'a> LayoutBuilder<'a> {
    fn new(cfg: &'a MarkdownLayoutConfig) -> Self {
        Self {
            cfg,
            blocks: Vec::new(),
            pending_links: Vec::new(),
            pending_rects: Vec::new(),
        }
    }

    fn finish(self) -> (DisplayLayout, Vec<RectOverlay>, MarkdownLinks) {
        let LayoutBuilder { cfg, blocks, pending_links, pending_rects } = self;
        let layout = EBlock::layout(
            &blocks,
            BlockLayoutConfig {
                line_height: cfg.line_height,
                char_width: cfg.char_width,
                baseline_offset: cfg.baseline_offset,
                default_fg: cfg.theme.body_fg,
                default_wrap_chars: cfg.wrap_chars,
            },
        );
        // Content right edge in line-local pixels = the wrap budget
        // (in chars) × char_width. Without a wrap budget, fall back to a
        // sentinel that keeps full-width overlays extending to the
        // viewport edge — matches the engine's `f32::MAX` convention.
        let content_right = cfg
            .wrap_chars
            .map(|c| c as f32 * cfg.char_width)
            .unwrap_or(f32::MAX);
        let ctx = OverlayContext {
            theme: &cfg.theme,
            content_right,
        };
        let overlays = resolve_pending_rects(&layout, &pending_rects, &ctx);
        let links = MarkdownLinks {
            spans: resolve_link_spans(&layout, &pending_links),
        };
        (layout, overlays, links)
    }

    fn indent_px(&self, depth: u32) -> f32 {
        depth as f32 * self.cfg.theme.indent_step
    }

    /// Dispatch table for block kinds. Keep variant arms one-liners;
    /// real work lives in `emit_*` helpers below.
    fn emit_block(&mut self, block: &Block, depth: u32, is_last: bool) {
        match block {
            Block::Heading { level, inlines } => self.emit_heading(*level, inlines, depth),
            Block::Paragraph(inlines) => self.emit_paragraph(inlines, depth, is_last),
            Block::BlockQuote(inner) => self.emit_blockquote(inner, depth),
            Block::List { ordered, start, items } => self.emit_list(*ordered, *start, items, depth),
            Block::CodeBlock { text, .. } => self.emit_code_block(text, depth),
            Block::Rule => self.emit_rule(depth),
        }
    }

    fn emit_blocks(&mut self, blocks: &[Block], depth: u32) {
        let last = blocks.len().saturating_sub(1);
        for (i, block) in blocks.iter().enumerate() {
            self.emit_block(block, depth, i == last);
        }
    }

    fn emit_heading(&mut self, level: u8, inlines: &[Inline], depth: u32) {
        let lvl_ix = (level as usize - 1).min(5);
        let scale = self.cfg.theme.heading_scale[lvl_ix];
        let fg = self.cfg.theme.heading_fg[lvl_ix];
        let row_height = self.cfg.line_height * scale;

        let block_idx = self.blocks.len();
        let (text, mut runs) = self.render_inlines(inlines, Some(block_idx));
        for r in &mut runs {
            r.fg = fg;
            r.font_scale = scale;
            r.font_weight = Some(self.cfg.theme.heading_weight);
        }
        // Plain headings come back with no runs — emit a single run so
        // the heading scale still applies to the whole row.
        if runs.is_empty() && !text.is_empty() {
            runs.push(StyleRun {
                byte_range: 0..text.len(),
                fg,
                bg: None,
                font_scale: scale,
                skew: 0.0,
                corner_radius: 0.0,
                font_weight: Some(self.cfg.theme.heading_weight),
                italic: false,
                font: None,
                decoration: None,
                link: None,
            });
        }

        self.blocks.push(
            EBlock::new(text)
                .with_runs(runs)
                .with_line_height(row_height)
                .with_padding(
                    self.cfg.theme.heading_padding_top,
                    self.cfg.theme.heading_padding_bottom,
                )
                .with_indent(self.indent_px(depth)),
        );
    }

    fn emit_paragraph(&mut self, inlines: &[Inline], depth: u32, is_last: bool) {
        let block_idx = self.blocks.len();
        let (text, runs) = self.render_inlines(inlines, Some(block_idx));
        let pad_bottom = if is_last {
            0.0
        } else {
            self.cfg.theme.paragraph_padding_bottom
        };
        let mut block = EBlock::new(text).with_indent(self.indent_px(depth));
        if !runs.is_empty() {
            block = block.with_runs(runs);
        }
        if pad_bottom > 0.0 {
            block = block.with_padding(0.0, pad_bottom);
        }
        self.blocks.push(block);
    }

    fn emit_blockquote(&mut self, inner: &[Block], depth: u32) {
        let start = self.blocks.len();
        self.emit_blocks(inner, depth + 1);
        let len = self.blocks.len() - start;
        if len == 0 {
            return;
        }
        // Left bar via per-row overlay: anchor a thin x-range rect at the
        // quote's indent on every row inside the quote. RowVertical::Full
        // picks up each row's actual line_height, so headings + multi-line
        // paragraphs inside the quote all get a uniform bar.
        self.pending_rects.push(PendingRect {
            block_idx: start,
            coverage: RectCoverage::Span { len },
            builder: blockquote_bar_overlay,
            aux: 0.0,
        });
    }

    fn emit_list(&mut self, ordered: bool, start: u64, items: &[Vec<Block>], depth: u32) {
        for (idx, item_blocks) in items.iter().enumerate() {
            let marker = if ordered {
                format!("{}. ", start as usize + idx)
            } else {
                "• ".to_string()
            };
            self.emit_list_item(&marker, item_blocks, depth);
        }
    }

    fn emit_list_item(&mut self, marker: &str, item_blocks: &[Block], depth: u32) {
        let inner_indent = depth + 1;
        let outer_indent_px = self.indent_px(depth);
        let start = self.blocks.len();
        self.emit_blocks(item_blocks, inner_indent);
        if let Some(first) = self.blocks.get_mut(start) {
            prepend_marker(first, marker, self.cfg.theme.body_fg);
            // The marker block sits at the outer indent so the bullet
            // visually leads its content (which is at inner_indent).
            first.indent = outer_indent_px;
        }
    }

    fn emit_code_block(&mut self, text: &str, depth: u32) {
        let pad = self.cfg.theme.code_block_padding;
        let fg = self.cfg.theme.decor.code_block_fg;
        let indent = self.indent_px(depth);

        let lines: Vec<&str> = text.split('\n').collect();
        let last = lines.len().saturating_sub(1);
        let start = self.blocks.len();
        for (i, line) in lines.iter().enumerate() {
            let mut block = code_block_line(*line, fg, indent, self.cfg.code_font.clone());
            // Padding is per-block: only the first row pays padding_top,
            // only the last row pays padding_bottom. Between rows, no
            // gap so the per-row overlay paints a continuous panel.
            if i == 0 {
                block = block.with_padding(pad, if i == last { pad } else { 0.0 });
            } else if i == last {
                block = block.with_padding(0.0, pad);
            }
            self.blocks.push(block);
        }
        self.pending_rects.push(PendingRect {
            block_idx: start,
            coverage: RectCoverage::Span {
                len: lines.len(),
            },
            builder: code_block_overlay,
            aux: 0.0,
        });
    }

    fn emit_rule(&mut self, depth: u32) {
        // The rule is a single empty block anchored to one display row
        // with vertical padding above and below. The thin line itself
        // is drawn as a row overlay so it scrolls with the content.
        let block_idx = self.blocks.len();
        self.blocks.push(
            EBlock::new(String::new())
                .with_indent(self.indent_px(depth))
                .with_padding(self.cfg.theme.rule_padding, self.cfg.theme.rule_padding),
        );
        self.pending_rects.push(PendingRect {
            block_idx,
            coverage: RectCoverage::SingleBlock,
            builder: rule_overlay,
            aux: 0.0,
        });
    }

    /// Walk inlines and produce `(text, runs)`. Pass `link_block_idx =
    /// Some(i)` to record `Inline::Link` byte ranges against engine
    /// block index `i` (the caller pushes the engine block at that
    /// index right after this returns).
    fn render_inlines(
        &mut self,
        inlines: &[Inline],
        link_block_idx: Option<usize>,
    ) -> (String, Vec<StyleRun>) {
        let mut buf = InlineBuffer::default();
        let mut state = InlineState {
            link_block_idx,
            ..Default::default()
        };
        self.walk_inlines(inlines, &mut buf, &mut state);
        (buf.text, buf.runs)
    }

    /// Dispatch table for inline kinds. One helper per variant.
    fn walk_inlines(&mut self, inlines: &[Inline], buf: &mut InlineBuffer, state: &mut InlineState) {
        for inline in inlines {
            match inline {
                Inline::Text(s) => self.inline_text(s, buf, state),
                Inline::Code(s) => self.inline_code(s, buf, state),
                Inline::Strong(inner) => self.inline_strong(inner, buf, state),
                Inline::Emph(inner) => self.inline_emph(inner, buf, state),
                Inline::Strike(inner) => self.inline_strike(inner, buf, state),
                Inline::Link { text, url } => self.inline_link(text, url, buf, state),
                Inline::HardBreak => buf.text.push('\n'),
            }
        }
    }

    fn inline_text(&self, s: &str, buf: &mut InlineBuffer, state: &InlineState) {
        if s.is_empty() {
            return;
        }
        let fg = if state.link_active {
            self.cfg.theme.link_fg
        } else {
            self.cfg.theme.body_fg
        };
        buf.push_run(s, fg, None, 0.0, state, self.cfg.theme.strong_weight, None);
    }

    fn inline_code(&self, s: &str, buf: &mut InlineBuffer, state: &InlineState) {
        let decor = &self.cfg.theme.decor;
        buf.push_run(
            s.trim(),
            decor.inline_code_fg,
            Some(decor.inline_code_bg),
            decor.inline_code_corner_radius,
            state,
            self.cfg.theme.strong_weight,
            self.cfg.code_font.clone(),
        );
    }

    fn inline_strong(&mut self, inner: &[Inline], buf: &mut InlineBuffer, state: &mut InlineState) {
        with_state(state, |s| s.bold = true, |state| self.walk_inlines(inner, buf, state));
    }

    fn inline_emph(&mut self, inner: &[Inline], buf: &mut InlineBuffer, state: &mut InlineState) {
        with_state(state, |s| s.italic = true, |state| self.walk_inlines(inner, buf, state));
    }

    fn inline_strike(&mut self, inner: &[Inline], buf: &mut InlineBuffer, state: &mut InlineState) {
        with_state(state, |s| s.strike = true, |state| self.walk_inlines(inner, buf, state));
    }

    fn inline_link(
        &mut self,
        inner: &[Inline],
        url: &str,
        buf: &mut InlineBuffer,
        state: &mut InlineState,
    ) {
        let url_arc: Arc<str> = Arc::from(url);
        let prev_link = state.link.clone();
        let prev_active = state.link_active;
        state.link = Some(url_arc.clone());
        state.link_active = true;

        let start = buf.text.len();
        self.walk_inlines(inner, buf, state);
        let end = buf.text.len();

        if let Some(block_idx) = state.link_block_idx {
            self.pending_links.push(PendingLink {
                block_idx,
                byte_range: start..end,
                url: url_arc,
            });
        }

        state.link = prev_link;
        state.link_active = prev_active;
    }
}

/// Run a closure inside a temporarily-mutated `InlineState` and restore
/// the previous state afterward. Centralizes the prev/restore boilerplate
/// so each inline-style helper is one line.
fn with_state(
    state: &mut InlineState,
    set: impl FnOnce(&mut InlineState),
    body: impl FnOnce(&mut InlineState),
) {
    let snapshot = state.snapshot();
    set(state);
    body(state);
    state.restore(snapshot);
}

/// Building a single block's text + runs. Keeps `walk_inlines` focused
/// on dispatch instead of byte-range bookkeeping.
#[derive(Default)]
struct InlineBuffer {
    text: String,
    runs: Vec<StyleRun>,
}

impl InlineBuffer {
    /// Append `s` and emit a `StyleRun` covering it. `fg`, `bg`, and
    /// `corner_radius` are caller-supplied; weight / italic / decoration /
    /// link come from the active `InlineState`.
    fn push_run(
        &mut self,
        s: &str,
        fg: bevy::prelude::Color,
        bg: Option<bevy::prelude::Color>,
        corner_radius: f32,
        state: &InlineState,
        strong_weight: u16,
        font: Option<Handle<Font>>,
    ) {
        let start = self.text.len();
        self.text.push_str(s);
        let end = self.text.len();
        if start == end {
            return;
        }
        self.runs.push(StyleRun {
            byte_range: start..end,
            fg,
            bg,
            font_scale: 0.0,
            skew: 0.0,
            corner_radius,
            font_weight: state.weight(strong_weight),
            italic: state.italic,
            font,
            decoration: state.decoration(),
            link: state.link.clone(),
        });
    }
}

/// Active inline-styling stack. A single `StyleRun` collapses multiple
/// nested marks (bold inside italic inside link) so we accumulate state
/// at leaf-text emission time rather than emitting per marker tag.
#[derive(Default, Clone)]
struct InlineState {
    bold: bool,
    italic: bool,
    strike: bool,
    link_active: bool,
    link: Option<Arc<str>>,
    /// When set, link-text byte ranges are recorded against this engine
    /// block index for the `MarkdownLinks` side channel.
    link_block_idx: Option<usize>,
}

/// Snapshot for `with_state` — the fields that vary across inline
/// nesting. `link_block_idx` is invariant within one inline walk.
#[derive(Clone)]
struct InlineStateSnap {
    bold: bool,
    italic: bool,
    strike: bool,
    link_active: bool,
    link: Option<Arc<str>>,
}

impl InlineState {
    fn snapshot(&self) -> InlineStateSnap {
        InlineStateSnap {
            bold: self.bold,
            italic: self.italic,
            strike: self.strike,
            link_active: self.link_active,
            link: self.link.clone(),
        }
    }

    fn restore(&mut self, s: InlineStateSnap) {
        self.bold = s.bold;
        self.italic = s.italic;
        self.strike = s.strike;
        self.link_active = s.link_active;
        self.link = s.link;
    }

    fn weight(&self, strong_weight: u16) -> Option<u16> {
        if self.bold {
            Some(strong_weight)
        } else {
            None
        }
    }

    fn decoration(&self) -> Option<TextDecoration> {
        if self.strike {
            Some(TextDecoration::Strikethrough)
        } else if self.link_active {
            Some(TextDecoration::Underline)
        } else {
            None
        }
    }
}

/// Resolve `(block_idx, byte_range)` link records against the laid-out
/// `DisplayLayout`. A link split across wrapped rows degrades to its
/// first row only; the side channel doesn't currently carry multi-row
/// spans.
fn resolve_link_spans(layout: &DisplayLayout, pending: &[PendingLink]) -> Vec<LinkSpan> {
    let mut out = Vec::with_capacity(pending.len());
    for link in pending {
        let buffer_row = link.block_idx as u32;
        for line in layout.lines.iter().filter(|l| l.buffer_row == buffer_row) {
            let row_start = line.buffer_byte_offset;
            let row_end = row_start + line.text.len();
            if link.byte_range.start >= row_end || link.byte_range.end <= row_start {
                continue;
            }
            let local_start = link.byte_range.start.saturating_sub(row_start);
            let local_end = link.byte_range.end.min(row_end) - row_start;
            out.push(LinkSpan {
                display_row: line.display_row,
                byte_range: local_start..local_end,
                url: link.url.clone(),
            });
            break;
        }
    }
    out
}

/// Build a single line of a fenced code block (no decoration applied yet).
fn code_block_line(line: &str, fg: bevy::prelude::Color, indent: f32, font: Option<Handle<Font>>) -> EBlock {
    let len = line.len();
    EBlock::new(line.to_string())
        .with_indent(indent)
        .with_runs(vec![StyleRun {
            byte_range: 0..len,
            fg,
            bg: None,
            font_scale: 0.0,
            skew: 0.0,
            corner_radius: 0.0,
            font_weight: None,
            italic: false,
            font,
            decoration: None,
            link: None,
        }])
}

/// Resolve pending row-anchored decoration rects against the laid-out
/// `DisplayLayout`. Each `PendingRect` knows the engine block it came
/// from and how far the coverage extends; this turns those into one
/// `RectOverlay` per display row.
///
/// The overlay path is row-anchored, so the renderer's
/// `push_overlay_quad` reads each row's `y_top` directly — this means
/// scroll-shifting the layout (the markdown viewer's per-frame trick)
/// places overlays correctly without re-emission.
fn resolve_pending_rects(
    layout: &DisplayLayout,
    pending: &[PendingRect],
    ctx: &OverlayContext,
) -> Vec<RectOverlay> {
    let mut out = Vec::with_capacity(pending.len());
    for p in pending {
        let last_block = match p.coverage {
            RectCoverage::SingleBlock => p.block_idx,
            RectCoverage::Span { len } => p.block_idx + len.saturating_sub(1),
        };
        let aux = p.aux;
        for_each_row_in_buffer_span(
            layout,
            p.block_idx as u32,
            last_block as u32,
            |display_row, pos| {
                out.push((p.builder)(display_row, pos, ctx, aux));
            },
        );
    }
    out
}

/// Code-block background overlay — tints the row from the line's left
/// edge out to `content_right`, capping at the wrap budget rather than
/// the viewport edge. Matches GitHub / VS Code: the panel reads as
/// content-width regardless of how wide the window is. `FullLeaded`
/// spans the full row height so adjacent rows paint a continuous panel
/// with no line-spacing gap between them. `corners_for_row` rounds only
/// the outer corners — first row's top, last row's bottom — so the
/// stack of per-row quads reads as a single rounded panel.
fn code_block_overlay(display_row: u32, pos: RowPosition, ctx: &OverlayContext, _aux: f32) -> RectOverlay {
    RectOverlay {
        display_row,
        x_range: 0.0..ctx.content_right,
        vertical: RowVertical::FullLeaded,
        color: ctx.theme.decor.code_block_bg,
        z: DECORATION_Z,
        corners: pos.corners(ctx.theme.decor.code_corner_radius),
    }
}

/// Blockquote left-bar overlay. Anchored to the row's left edge with a
/// fixed pixel width; `FullLeaded` so the bar reads as one continuous
/// line across multi-paragraph quotes rather than dashing per row.
/// Sharp corners — the bar reads as a clean rule, not a pill.
/// `aux` is unused (0.0). The bar sits one `indent_step` to the left of
/// the text — the renderer adds `line.x_offset` (inner indent), so
/// `x_range.start = -indent_step` lands at the outer indent level.
fn blockquote_bar_overlay(display_row: u32, _pos: RowPosition, ctx: &OverlayContext, _aux: f32) -> RectOverlay {
    let x_start = -ctx.theme.indent_step;
    RectOverlay {
        display_row,
        x_range: x_start..x_start + ctx.theme.blockquote_bar_width,
        vertical: RowVertical::FullLeaded,
        color: ctx.theme.decor.blockquote_bar,
        z: BLOCKQUOTE_BAR_Z,
        corners: bevy_instanced_text::CornerRadii::ZERO,
    }
}

/// Horizontal-rule overlay — a thin band across the row's mid-height,
/// capped at the content width. Single-row by construction; sharp.
fn rule_overlay(display_row: u32, _pos: RowPosition, ctx: &OverlayContext, _aux: f32) -> RectOverlay {
    RectOverlay {
        display_row,
        x_range: 0.0..ctx.content_right,
        vertical: RowVertical::TopBand {
            thickness: ctx.theme.rule_thickness,
        },
        color: ctx.theme.decor.rule_color,
        z: DECORATION_Z,
        corners: bevy_instanced_text::CornerRadii::ZERO,
    }
}


/// Prepend a list marker into an engine block: shifts existing run
/// byte_ranges by `marker.len()` and inserts a leading body-styled run
/// covering the marker.
fn prepend_marker(block: &mut EBlock, marker: &str, body_fg: bevy::prelude::Color) {
    let marker_len = marker.len();
    let mut new_text = String::with_capacity(marker_len + block.text.len());
    new_text.push_str(marker);
    new_text.push_str(&block.text);

    let mut new_runs = Vec::with_capacity(block.runs.len() + 1);
    new_runs.push(StyleRun {
        byte_range: 0..marker_len,
        fg: body_fg,
        bg: None,
        font_scale: 0.0,
        skew: 0.0,
        corner_radius: 0.0,
        font_weight: None,
        italic: false,
        font: None,
        decoration: None,
        link: None,
    });
    for r in block.runs.drain(..) {
        let mut shifted = r;
        shifted.byte_range =
            (shifted.byte_range.start + marker_len)..(shifted.byte_range.end + marker_len);
        new_runs.push(shifted);
    }
    block.text = new_text;
    block.runs = new_runs;
}
