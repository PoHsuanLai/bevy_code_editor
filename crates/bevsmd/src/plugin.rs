//! Plugin and supporting types for the markdown viewer.

use std::sync::Arc;

use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;
use bevy_instanced_text::{
    ContentMetrics, DisplayLayout, FontConfig, RectOverlay, ScrollState, ShapedLine, TextBuffer,
    TextView, TextViewOverlays, TextViewViewport,
};

/// Size and scroll geometry for a markdown viewer entity.
pub type MarkdownViewport = TextViewViewport;

/// Font path, size, and line-height for a markdown viewer entity.
pub type MarkdownFont = FontConfig;

use crate::layout::{layout_markdown, MarkdownLayoutConfig};
use crate::parse::parse_markdown;
use crate::theme::MarkdownTheme;
use crate::view::{MarkdownCodeFont, MarkdownDoc, MarkdownLinks};

/// Cached pre-scroll layout — written by the rebuild system, read by the
/// scroll system. Not part of the public API.
#[derive(Component, Clone)]
pub(crate) struct BaseMarkdownLayout {
    pub(crate) layout: DisplayLayout,
    pub(crate) base_overlays: Vec<RectOverlay>,
}

/// Adds the markdown parse-on-change and scroll systems to the app.
/// Use [`MarkdownViewerPlugins`] to also pull in the rendering layer.
pub struct MarkdownViewerPlugin;

impl Plugin for MarkdownViewerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (rebuild_markdown_layout, apply_markdown_scroll).chain(),
        );
    }
}

/// Full plugin bundle for embedding a markdown viewer. Adds the rendering
/// layer plus [`MarkdownViewerPlugin`]. Use this unless you are already
/// managing the rendering plugins yourself.
pub struct MarkdownViewerPlugins;

impl bevy::app::PluginGroup for MarkdownViewerPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(bevy_instanced_text::gpu::GlyphAtlasPlugin)
            .add(bevy_instanced_text::gpu::InstancedTextRenderPlugin)
            .add(bevy_instanced_text::view::plugin::InstancedTextPlugin)
            .add(MarkdownViewerPlugin)
    }
}

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
            Option<&MarkdownCodeFont>,
            Option<&mut TextBuffer>,
            Option<&mut ContentMetrics>,
        ),
        Or<(
            Changed<MarkdownDoc>,
            Changed<MarkdownTheme>,
            Changed<FontConfig>,
            Changed<TextViewViewport>,
            Changed<MarkdownCodeFont>,
        )>,
    >,
) {
    for (entity, doc, theme, font, viewport, code_font, buffer, metrics) in q.iter_mut() {
        let blocks = parse_markdown(&doc.source);
        let mut cfg = MarkdownLayoutConfig::from_metrics(
            theme.clone(),
            font.line_height,
            font.char_width,
            font.font_size * 0.32,
            Some(content_width_px(viewport)),
        );
        cfg.code_font = code_font.map(|c| c.0.clone());
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

#[allow(clippy::type_complexity)]
fn apply_markdown_scroll(
    mut commands: Commands,
    q: Query<
        (Entity, &ScrollState, &TextViewViewport, &BaseMarkdownLayout),
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

fn content_width_px(viewport: &TextViewViewport) -> f32 {
    let left = if viewport.gutter_width > 0.0 {
        viewport.text_area_left.max(viewport.gutter_width)
    } else {
        viewport.text_area_left
    };
    let right_margin = viewport.text_area_left;
    (viewport.width as f32 - left - right_margin).max(80.0)
}

/// Everything a markdown viewer entity needs. Spawn this and the plugin manages the rest.
///
/// ```rust,no_run
/// # use bevy::prelude::*;
/// # use bevsmd::prelude::*;
/// # fn setup(mut commands: Commands) {
/// commands.spawn(MarkdownViewerBundle {
///     doc: MarkdownDoc::new("# Hello"),
///     viewport: MarkdownViewport { width: 800, height: 600, ..default() },
///     font: MarkdownFont::from_size(16.0),
///     ..default()
/// });
/// # }
/// ```
#[derive(Bundle, Default)]
pub struct MarkdownViewerBundle {
    pub view: TextView,
    pub buffer: TextBuffer,
    pub scroll: ScrollState,
    pub metrics: ContentMetrics,
    pub viewport: MarkdownViewport,
    pub font: MarkdownFont,
    pub doc: MarkdownDoc,
    pub layout: DisplayLayout,
    pub overlays: TextViewOverlays,
    pub links: MarkdownLinks,
}
