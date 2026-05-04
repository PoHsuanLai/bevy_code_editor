//! Generic text view rendering — produces `GlyphInstance` batches from a
//! `DisplayLayout` and (optional) `TextViewOverlays`.
//!
//! Pure function over an immutable snapshot: no cursors, selections, syntax
//! highlighting, or other editor concepts here. The producer (`display_map`)
//! has already shaped each visible row through cosmic-text and resolved
//! per-line/per-run colors into the `ShapedLine.runs`. This module turns
//! that into per-glyph quads ready for the instanced pipeline.

use bevy::prelude::*;

use crate::gpu::GlyphAtlas;

use super::layout::{line_x_at_byte, DisplayLayout};
use super::overlay::TextViewOverlays;
use super::snapshot::ShapedLine;
use super::viewport::TextViewViewport;

/// Glyph instance data for GPU rendering
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct GlyphInstance {
    pub position: Vec2,
    pub uv_min: Vec2,
    pub uv_max: Vec2,
    pub size: Vec2,
    pub color: [f32; 4],
    pub z_index: f32,
    /// Corner radius in pixels (0 = sharp corners, used for background rects)
    pub corner_radius: f32,
    /// Horizontal skew factor for italic simulation (0.0 = normal, ~0.2 = italic)
    pub skew: f32,
    pub _padding: f32,
}

/// Marker component for a text view's GPU batch entity
#[derive(Component)]
pub struct TextViewBatch {
    /// The scroll offset when this batch was built
    pub built_at_scroll: f32,
    pub built_at_horizontal_scroll: f32,
    /// The visible line range when built
    pub first_line: usize,
    pub last_line: usize,
    /// Viewport dimensions when built
    pub built_at_width: u32,
    pub built_at_height: u32,
}

/// Component containing batch of glyph instances for GPU rendering
#[derive(Component, Clone)]
pub struct GlyphBatchComponent {
    pub instances: Vec<GlyphInstance>,
    pub atlas_texture: Handle<Image>,
    /// Which render layer this batch belongs to (for multi-viewport filtering).
    /// None = layer 0 (default).
    pub render_layer: Option<u8>,
}

/// Render a `DisplayLayout` into glyph instances.
///
/// This is the new path: the renderer is a pure function over an immutable snapshot.
/// The caller (display_map) has already done folding, soft-wrap, viewport culling,
/// and styling — every `ShapedLine` in `layout.lines` is in display order, ready to paint.
///
/// Overlays (cursor, selection, line highlight, brackets) are layered in via `RectOverlay`.
/// Negative-z rects render below text; positive-z above.
pub fn render_layout(
    layout: &DisplayLayout,
    overlays: Option<&TextViewOverlays>,
    viewport: &TextViewViewport,
    atlas: &mut GlyphAtlas,
    content_start_x: f32,
    horizontal_scroll_offset: f32,
    font_size: f32,
) -> Vec<GlyphInstance> {
    let default_line_height = layout.line_height;
    let char_width = layout.char_width;
    let baseline_offset = layout.baseline_offset;

    let viewport_world_left = viewport.world_left();
    let viewport_world_top = viewport.world_top();
    let line_start_x = content_start_x - horizontal_scroll_offset;

    // Single anchor: `line.y_top` is the row's visual top in screen pixels.
    //   - Glyph baseline = y_top + line_height/2 + baseline_offset (legacy ascent ratio).
    //   - Overlay full-line rect spans y_top..y_top+line_height.
    //   - Sub-line overlays (`y_range: Some(0..2)`) are y_top-relative.
    // Per-row overrides: `ShapedLine.line_height = Some(h)` lets a row paint
    // taller / shorter than the layout default (markdown headings, code blocks).

    let mut text_instances: Vec<GlyphInstance> = Vec::with_capacity(layout.lines.len() * 80);
    let mut below_instances: Vec<GlyphInstance> = Vec::new();
    let mut above_instances: Vec<GlyphInstance> = Vec::new();

    // Below-text overlays first (selection bg, line highlight)
    if let Some(ovs) = overlays {
        for rect in ovs.rects.iter().filter(|r| r.z < 0) {
            push_overlay_quad(
                rect,
                layout,
                viewport_world_left,
                viewport_world_top,
                line_start_x,
                atlas.solid_uv,
                &mut below_instances,
            );
        }
    }

    // Glyphs and per-line/per-run backgrounds
    for line in layout.lines.iter() {
        let line_height = line.line_height.unwrap_or(default_line_height);
        // Glyph baseline derived from row top.
        let base_y = line.y_top + line_height * 0.5 + baseline_offset;
        let line_x = line_start_x + line.x_offset;


        // Line background (full-width quad) — full row, top-anchored on y_top.
        if let Some(bg) = line.line_bg {
            let margin = content_start_x;
            let bg_pad = if line.x_offset > 0.0 {
                char_width * 1.5
            } else {
                0.0
            };
            let bg_x_start = (line.x_offset - bg_pad).max(0.0);
            // Pick the largest corner radius among runs sharing this background.
            let line_corner_radius = line
                .runs
                .iter()
                .filter(|r| r.bg == Some(bg))
                .map(|r| r.corner_radius)
                .fold(0.0_f32, f32::max);
            below_instances.push(GlyphInstance {
                position: Vec2::new(
                    viewport_world_left + margin + bg_x_start,
                    viewport_world_top - line.y_top - line_height * 0.5,
                ),
                uv_min: atlas.solid_uv.uv_min,
                uv_max: atlas.solid_uv.uv_max,
                size: Vec2::new(
                    viewport.width as f32 - margin * 2.0 - bg_x_start,
                    line_height,
                ),
                color: linear_rgba(bg),
                z_index: 0.0,
                corner_radius: line_corner_radius,
                skew: 0.0,
                _padding: 0.0,
            });
        }

        // Pre-compose the row anchor; threads through every emitter call.
        let anchor = RowAnchor {
            viewport_world_left,
            viewport_world_top,
            line_x,
            base_y,
        };

        // Shape-driven path is used when the line carries a `LineShape` shaped
        // at this font_size and no run wants a font_scale override. Runs with
        // `font_scale != 0.0/1.0` need re-shaping at a different size, which
        // happens on demand inside `emit_unshaped_run_glyphs`.
        let shape_usable = line
            .shape
            .as_ref()
            .filter(|s| (s.font_size - font_size).abs() < f32::EPSILON)
            .filter(|_| line.runs.iter().all(|r| r.font_scale == 0.0 || r.font_scale == 1.0));

        if line.runs.is_empty() {
            let style = RunStyle {
                color: linear_rgba(layout.default_fg),
                skew: 0.0,
            };
            if let Some(shape) = shape_usable {
                emit_shaped_run_glyphs(
                    &shape.glyphs,
                    0..line.text.len(),
                    anchor,
                    style,
                    atlas,
                    &mut text_instances,
                );
            } else {
                emit_unshaped_run_glyphs(
                    line,
                    0..line.text.len(),
                    anchor,
                    style,
                    RunMetrics {
                        font_size,
                        start_x: 0.0,
                    },
                    atlas,
                    &mut text_instances,
                );
            }
        } else {
            for run in &line.runs {
                if line.text.get(run.byte_range.clone()).is_none() {
                    continue;
                }

                let seg_x_start = line_byte_to_x(line, run.byte_range.start, char_width);
                let seg_x_end = line_byte_to_x(line, run.byte_range.end, char_width);

                // Per-run background (skip if it duplicates the line bg)
                if let Some(bg) = run.bg {
                    if line.line_bg != Some(bg) {
                        below_instances.push(GlyphInstance {
                            position: Vec2::new(
                                viewport_world_left + line_x + seg_x_start,
                                viewport_world_top - line.y_top - line_height * 0.5,
                            ),
                            uv_min: atlas.solid_uv.uv_min,
                            uv_max: atlas.solid_uv.uv_max,
                            size: Vec2::new((seg_x_end - seg_x_start).max(0.0), line_height),
                            color: linear_rgba(bg),
                            z_index: 0.0,
                            corner_radius: run.corner_radius,
                            skew: 0.0,
                            _padding: 0.0,
                        });
                    }
                }

                let style = RunStyle {
                    color: linear_rgba(run.fg),
                    skew: run.skew,
                };
                let seg_font_size = if run.font_scale > 0.0 {
                    font_size * run.font_scale
                } else {
                    font_size
                };

                if let Some(shape) = shape_usable {
                    emit_shaped_run_glyphs(
                        &shape.glyphs,
                        run.byte_range.clone(),
                        anchor,
                        style,
                        atlas,
                        &mut text_instances,
                    );
                } else {
                    emit_unshaped_run_glyphs(
                        line,
                        run.byte_range.clone(),
                        anchor,
                        style,
                        RunMetrics {
                            font_size: seg_font_size,
                            start_x: seg_x_start,
                        },
                        atlas,
                        &mut text_instances,
                    );
                }
            }
        }
    }

    // Above-text overlays (carets)
    if let Some(ovs) = overlays {
        for rect in ovs.rects.iter().filter(|r| r.z >= 0) {
            push_overlay_quad(
                rect,
                layout,
                viewport_world_left,
                viewport_world_top,
                line_start_x,
                atlas.solid_uv,
                &mut above_instances,
            );
        }
    }

    // Painter's order: below → text → above
    let mut out = below_instances;
    out.append(&mut text_instances);
    out.append(&mut above_instances);
    out
}

/// Pixel offset of `byte` within `line.text`. Uses shaped advances when present
/// and falls back to a `char_width` walk otherwise. Tab handling matches the
/// shaper's behavior (it inflates `\t` advance by `tab_width = 4`).
fn line_byte_to_x(line: &ShapedLine, byte: usize, char_width: f32) -> f32 {
    line_x_at_byte(line, byte, char_width)
}

/// Where a row paints — viewport origin + line-local offsets. Composed once
/// per row in `render_layout` and threaded into both glyph emitters and quad
/// pushers, eliminating repeated 4-float parameter lists.
#[derive(Clone, Copy)]
struct RowAnchor {
    /// Viewport's world-space top-left X (Bevy's center-origin Y is inverted).
    viewport_world_left: f32,
    /// Viewport's world-space top edge (Y inversion baseline).
    viewport_world_top: f32,
    /// Line-local origin X in screen pixels, includes scroll + line.x_offset.
    line_x: f32,
    /// Glyph baseline Y in screen pixels (top-down origin).
    base_y: f32,
}

/// Per-run paint attributes. `color` is pre-linearized; `skew` carries italic
/// simulation. `corner_radius: 0.0` since glyphs themselves don't round.
#[derive(Clone, Copy)]
struct RunStyle {
    color: [f32; 4],
    skew: f32,
}

/// Build a glyph quad from an atlas hit. Centralizes the legacy/shaped paint
/// math so both emitters share one expression for screen→world conversion.
fn glyph_quad(
    info: crate::gpu::GlyphInfo,
    pen_x: f32,
    anchor: RowAnchor,
    style: RunStyle,
) -> GlyphInstance {
    let screen_x = anchor.line_x + pen_x + info.offset.x;
    let screen_y = anchor.base_y - info.offset.y;
    GlyphInstance {
        position: Vec2::new(
            anchor.viewport_world_left + screen_x,
            anchor.viewport_world_top - screen_y - info.size.y,
        ),
        uv_min: info.uv_min,
        uv_max: info.uv_max,
        size: info.size,
        color: style.color,
        z_index: 0.0,
        corner_radius: 0.0,
        skew: style.skew,
        _padding: 0.0,
    }
}

/// Emit glyphs whose `byte_index` lies inside `range` from a pre-shaped line.
/// `glyphs[i].x` is line-local; `glyph_quad` adds the row anchor.
fn emit_shaped_run_glyphs(
    glyphs: &[super::snapshot::ShapedGlyph],
    range: std::ops::Range<usize>,
    anchor: RowAnchor,
    style: RunStyle,
    atlas: &mut GlyphAtlas,
    out: &mut Vec<GlyphInstance>,
) {
    for g in glyphs {
        if g.byte_index < range.start || g.byte_index >= range.end {
            continue;
        }
        let Some((info, _)) = atlas.get_or_rasterize_glyph(g.cache_key) else {
            continue;
        };
        out.push(glyph_quad(info, g.x, anchor, style));
    }
}

/// Per-run metrics for the shape-on-demand fallback path. `font_size` is the
/// rasterizer size (may differ from the line default when a `font_scale` run
/// overrides); `start_x` is the run's pen-x relative to the line origin (where
/// it picks up after any preceding runs).
#[derive(Clone, Copy)]
struct RunMetrics {
    font_size: f32,
    start_x: f32,
}

/// Emit glyphs for a byte range by shaping it on demand. Used when no
/// `LineShape` is attached (`trivial_layout` consumers) or when a
/// `StyleRun.font_scale` override requires re-shaping at a different size.
///
/// Shaping a per-frame slice is cheap — it's the same code path the producer
/// uses, just narrowed to the run. For monospace ASCII it yields advances
/// byte-identical to the old `col * char_width` walk.
fn emit_unshaped_run_glyphs(
    line: &ShapedLine,
    range: std::ops::Range<usize>,
    anchor: RowAnchor,
    style: RunStyle,
    metrics: RunMetrics,
    atlas: &mut GlyphAtlas,
    out: &mut Vec<GlyphInstance>,
) {
    let Some(slice) = line.text.get(range) else {
        return;
    };
    // Strip a trailing newline — the rope line includes it but cosmic-text
    // would just emit a zero-advance glyph. Matches the producer's behavior.
    let shape_text = slice.strip_suffix('\n').unwrap_or(slice);
    let shape = atlas.shape_line(shape_text, metrics.font_size);
    for g in &shape.glyphs {
        let Some((info, _)) = atlas.get_or_rasterize_glyph(g.cache_key) else {
            continue;
        };
        out.push(glyph_quad(info, metrics.start_x + g.x, anchor, style));
    }
}

fn push_overlay_quad(
    rect: &super::overlay::RectOverlay,
    layout: &DisplayLayout,
    world_left: f32,
    world_top: f32,
    line_start_x: f32,
    solid_uv: crate::gpu::GlyphInfo,
    out: &mut Vec<GlyphInstance>,
) {
    let Some(line) = layout
        .lines
        .iter()
        .find(|l| l.display_row == rect.display_row)
    else {
        return;
    };
    let line_height = line.line_height.unwrap_or(layout.line_height);
    let baseline_offset = layout.baseline_offset;
    let x0 = rect.x_range.start.max(0.0);
    let x1 = if rect.x_range.end >= f32::MAX / 2.0 {
        // Full-line: extend to the viewport's right edge from `line_start_x + line.x_offset`.
        // The layout doesn't carry viewport width here, so callers using f32::MAX get a sentinel.
        x0 + 1.0
    } else {
        rect.x_range.end
    };
    let width = (x1 - x0).max(1.0);

    // Resolve semantic vertical placement against the row's geometry.
    // y_off is from the row's top edge (line.y_top); height is the rect height.
    //
    // Important: the row "box" (`y_top..y_top + line_height`) includes leading
    // above/below the actual glyph cap-to-descender extent. Carets and full-line
    // backgrounds should align with the *text* (centered on the baseline), not
    // with the leaded box, so they sit visually on the row of text rather than
    // straddling line-spacing whitespace.
    let baseline_y_off = line_height * 0.5 + baseline_offset;
    // Cap-to-descender extent ≈ ascent + descent, the visual "text band" within
    // the leaded line. Approximated from the same heuristic used for baseline_offset
    // (~32% of font size for descender-ish region above baseline). This gives a
    // tight box around glyphs, used as the canonical text-aligned span for
    // carets and full-line backgrounds.
    let cap_to_descender = baseline_y_off + baseline_offset * 0.6;
    // Bias text-aligned overlays slightly below the baseline so they cover the
    // descender region (where the eye perceives the text "sitting"), not just
    // float around the baseline midpoint. Used uniformly by Full + Caret so
    // selection backgrounds and carets stack to the same Y.
    let text_band_above_baseline = cap_to_descender * 0.25;
    let (y_off, height) = match rect.vertical {
        super::overlay::RowVertical::Full => {
            (baseline_y_off - text_band_above_baseline, cap_to_descender)
        }
        super::overlay::RowVertical::Caret { height_fraction } => {
            let h = (cap_to_descender * height_fraction).max(1.0);
            (baseline_y_off - h * 0.25, h)
        }
        super::overlay::RowVertical::TopBand { thickness } => (0.0, thickness.max(1.0)),
        super::overlay::RowVertical::BottomBand { thickness } => {
            let t = thickness.max(1.0);
            (line_height - t, t)
        }
        super::overlay::RowVertical::UnderBaseline { thickness, gap } => {
            (baseline_y_off + gap, thickness.max(1.0))
        }
    };

    let world_x = world_left + line_start_x + line.x_offset + x0;
    // Single anchor: line.y_top is the row's screen-Y top. Quad position is the
    // *center* of the rect; world_top inverts Y.
    let world_y = world_top - line.y_top - y_off - height * 0.5;


    out.push(GlyphInstance {
        position: Vec2::new(world_x, world_y),
        uv_min: solid_uv.uv_min,
        uv_max: solid_uv.uv_max,
        size: Vec2::new(width, height),
        color: linear_rgba(rect.color),
        z_index: 0.0,
        corner_radius: rect.corner_radius,
        skew: 0.0,
        _padding: 0.0,
    });
}

/// Pre-multiplied linear RGBA suitable for `GlyphInstance.color`.
fn linear_rgba(color: Color) -> [f32; 4] {
    let l = color.to_linear();
    [l.red, l.green, l.blue, l.alpha]
}
