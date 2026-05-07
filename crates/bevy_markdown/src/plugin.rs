//! `MarkdownViewerPlugin` — owns the parse-on-change and scroll-on-change
//! systems for entities carrying a `MarkdownDoc`.
//!
//! Two systems run in `Update`:
//!
//! - `rebuild_markdown_layout`: re-parses + re-lays out the document when
//!   the source text, font config, viewport, or theme changes. Stashes the
//!   pre-scroll layout on the entity as a `BaseMarkdownLayout` component.
//! - `apply_markdown_scroll`: shifts the cached layout's `y_top` values by
//!   `scroll_offset + text_area_top` whenever the entity's
//!   `ScrollState.scroll_offset` (or viewport offsets) change. Cheap —
//!   clones the `Arc<Vec<ShapedLine>>` and rewrites only `y_top`.
//!
//! The split keeps scrolling cheap: parse + flatten happen once; per-frame
//! scroll just rewrites floats.

use std::sync::Arc;

use bevy::prelude::*;
use bevy_text_engine::{
    ContentMetrics, DisplayLayout, FontConfig, RectOverlay, ScrollState, ShapedLine, TextBuffer,
    TextView, TextViewOverlays, TextViewViewport,
};

use crate::layout::{layout_markdown, MarkdownLayoutConfig};
use crate::parse::parse_markdown;
use crate::theme::MarkdownTheme;
use crate::view::{MarkdownDoc, MarkdownLinks};

/// Cached pre-scroll layout. The rebuild system writes this; the scroll
/// system reads it and produces the per-frame `DisplayLayout`. Stored
/// separately so re-laying out doesn't churn the actual `DisplayLayout`
/// Arc seen by the renderer when scroll changes.
///
/// Also caches the row-anchored decoration overlays (`base_overlays`)
/// alongside the layout. The scroll system copies these into a
/// `TextViewOverlays` component each frame; the renderer picks them up
/// via the same anchor mechanism used for selection / cursors, so
/// scrolling shifts decorations naturally with their host rows.
#[derive(Component, Clone)]
pub struct BaseMarkdownLayout {
    pub layout: DisplayLayout,
    pub base_overlays: Vec<RectOverlay>,
}

/// Adds the markdown rebuild + scroll systems. Pair with `TextEnginePlugins`
/// for rendering.
pub struct MarkdownViewerPlugin;

impl Plugin for MarkdownViewerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (rebuild_markdown_layout, apply_markdown_scroll).chain(),
        );
    }
}

/// Re-parses the markdown source + rebuilds the engine block list when
/// the doc, font, viewport, or theme changes. Writes a `BaseMarkdownLayout`
/// on the entity (the un-scrolled layout) which the scroll system reads.
#[allow(clippy::type_complexity)]
fn rebuild_markdown_layout(
    mut commands: Commands,
    mut q: Query<
        (
            Entity,
            &MarkdownDoc,
            &MarkdownTheme,
            &FontConfig,
            &TextViewViewport,
            Option<&mut TextBuffer>,
            Option<&mut ContentMetrics>,
        ),
        Or<(
            Changed<MarkdownDoc>,
            Changed<MarkdownTheme>,
            Changed<FontConfig>,
            Changed<TextViewViewport>,
        )>,
    >,
) {
    for (entity, doc, theme, font, viewport, buffer, metrics) in q.iter_mut() {
        let blocks = parse_markdown(&doc.source);
        let cfg = MarkdownLayoutConfig::from_metrics(
            theme.clone(),
            font.line_height,
            font.char_width,
            font.font_size * 0.32,
            Some(content_width_px(viewport)),
        );
        let (layout, base_overlays, links) = layout_markdown(&blocks, &cfg);

        // Sync the rope so selection / copy reads the rendered text.
        // Source markdown stays in `MarkdownDoc.source`.
        if let Some(mut buffer) = buffer {
            let rendered: String = layout
                .lines
                .iter()
                .map(|l| l.text.clone())
                .collect::<Vec<_>>()
                .join("\n");
            buffer.rope = ropey::Rope::from_str(&rendered);
        }
        if let Some(mut metrics) = metrics {
            metrics.max_content_width = layout
                .lines
                .iter()
                .map(|l| l.shape.as_ref().map(|s| s.width).unwrap_or(0.0))
                .fold(0.0_f32, f32::max);
        }

        commands.entity(entity).insert((
            BaseMarkdownLayout {
                layout,
                base_overlays,
            },
            links,
        ));
    }
}

/// Produce the per-frame `DisplayLayout` by adding the entity's vertical
/// scroll offset + `text_area_top` to each row's `y_top`. Also re-emits
/// the row-anchored decoration overlays — anchored by `display_row`, the
/// renderer resolves their Y from the (now-shifted) `y_top` at paint
/// time, so we just copy them verbatim.
///
/// Runs on every frame the scroll or the base layout changed; cheap
/// because rows are re-emitted with only their `y_top` shifted (Vec
/// allocation, no shaping or restyling).
#[allow(clippy::type_complexity)]
fn apply_markdown_scroll(
    mut commands: Commands,
    q: Query<
        (
            Entity,
            &ScrollState,
            &TextViewViewport,
            &BaseMarkdownLayout,
        ),
        Or<(
            Changed<ScrollState>,
            Changed<TextViewViewport>,
            Changed<BaseMarkdownLayout>,
        )>,
    >,
) {
    for (entity, scroll, viewport, base) in q.iter() {
        let layout = shift_layout_y(&base.layout, viewport.text_area_top + scroll.scroll_offset);
        let overlays = TextViewOverlays {
            rects: base.base_overlays.clone(),
            // Bumping version is what tells the engine's render
            // skip-on-unchanged gate the overlays may need re-painting.
            version: 0,
        };
        commands.entity(entity).insert((layout, overlays));
    }
}

/// Clone a `DisplayLayout` and shift each row's `y_top` by `delta`. The
/// row data (`text`, `runs`, `shape`) is reused as-is since `Vec<ShapedLine>`
/// is wrapped in an Arc; we allocate a new Vec only on the float column.
fn shift_layout_y(base: &DisplayLayout, delta: f32) -> DisplayLayout {
    let lines: Vec<ShapedLine> = base
        .lines
        .iter()
        .map(|l| ShapedLine {
            y_top: l.y_top + delta,
            text: l.text.clone(),
            runs: l.runs.clone(),
            shape: l.shape.clone(),
            line_bg: l.line_bg,
            line_height: l.line_height,
            padding_top: l.padding_top,
            padding_bottom: l.padding_bottom,
            display_row: l.display_row,
            buffer_row: l.buffer_row,
            buffer_byte_offset: l.buffer_byte_offset,
            is_wrap_continuation: l.is_wrap_continuation,
            x_offset: l.x_offset,
        })
        .collect();
    DisplayLayout {
        lines: Arc::new(lines),
        block_rects: base.block_rects.clone(),
        visible_rows: base.visible_rows.clone(),
        total_display_rows: base.total_display_rows,
        line_height: base.line_height,
        char_width: base.char_width,
        baseline_offset: base.baseline_offset,
        default_fg: base.default_fg,
        version: base.version.wrapping_add(1),
        scroll_version: base.scroll_version.wrapping_add(1),
    }
}

/// Pixel budget for soft-wrap. Mirrors the renderer's content area:
/// `viewport.width - left_margin - right_margin`, where the right
/// margin matches `text_area_left` (the renderer's `push_block_decorations`
/// uses the same convention).
fn content_width_px(viewport: &TextViewViewport) -> f32 {
    let left = if viewport.gutter_width > 0.0 {
        viewport.text_area_left.max(viewport.gutter_width)
    } else {
        viewport.text_area_left
    };
    let right_margin = viewport.text_area_left;
    (viewport.width as f32 - left - right_margin).max(80.0)
}

/// Components a markdown viewer entity needs alongside `MarkdownDoc`.
/// Spawn this bundle to get the engine's scroll/selection/copy behavior
/// for free.
#[derive(Bundle, Default)]
pub struct MarkdownViewerBundle {
    pub view: TextView,
    pub buffer: TextBuffer,
    pub scroll: ScrollState,
    pub metrics: ContentMetrics,
    pub viewport: TextViewViewport,
    pub font: FontConfig,
    pub doc: MarkdownDoc,
    /// Filled in by `rebuild_markdown_layout` on first frame; spawn with
    /// `DisplayLayout::default()` so the engine's render skip-on-unchanged
    /// gate sees a layout immediately.
    pub layout: DisplayLayout,
    pub overlays: TextViewOverlays,
    pub links: MarkdownLinks,
}
