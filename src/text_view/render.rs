//! Generic text view rendering — produces GlyphInstance batches from TextViewState.
//!
//! This module contains the core rendering logic extracted from the editor's
//! `update_gpu_text_instanced` system. It knows nothing about cursors, selections,
//! syntax highlighting, or any other editor concept — it simply renders styled text
//! lines into a viewport.

use bevy::prelude::*;

use crate::gpu_text::{GlyphAtlas, GlyphKey, GlyphRasterizer};
use crate::settings::FontSettings;


use super::layout::DisplayLayout;
use super::overlay::TextViewOverlays;
use super::state::TextViewState;
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
    let line_height = layout.line_height;
    let char_width = layout.char_width;
    let baseline_offset = layout.baseline_offset;

    let viewport_world_left = viewport.world_left();
    let viewport_world_top = viewport.world_top();
    let line_start_x = content_start_x - horizontal_scroll_offset;

    // Single anchor: `line.y_top` is the row's visual top in screen pixels.
    //   - Glyph baseline = y_top + line_height/2 + baseline_offset (legacy ascent ratio).
    //   - Overlay full-line rect spans y_top..y_top+line_height.
    //   - Sub-line overlays (`y_range: Some(0..2)`) are y_top-relative.

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
        // Glyph baseline derived from row top.
        let base_y = line.y_top + line_height * 0.5 + baseline_offset;
        let line_x = line_start_x + line.x_offset;


        // Line background (full-width quad) — full row, top-anchored on y_top.
        if let Some(bg) = line.line_bg {
            let bg_linear = bg.to_linear();
            let margin = content_start_x;
            let bg_pad = if line.x_offset > 0.0 {
                char_width * 1.5
            } else {
                0.0
            };
            let bg_x_start = (line.x_offset - bg_pad).max(0.0);
            let bg_world_x = viewport_world_left + margin + bg_x_start;
            let bg_width = viewport.width as f32 - margin * 2.0 - bg_x_start;
            let bg_world_y = viewport_world_top - line.y_top - line_height * 0.5;
            // Pick the largest corner radius among runs that share this background, if any.
            let line_corner_radius = line
                .runs
                .iter()
                .filter(|r| r.bg == Some(bg))
                .map(|r| r.corner_radius)
                .fold(0.0_f32, f32::max);
            below_instances.push(GlyphInstance {
                position: Vec2::new(bg_world_x, bg_world_y),
                uv_min: atlas.solid_uv.uv_min,
                uv_max: atlas.solid_uv.uv_max,
                size: Vec2::new(bg_width, line_height),
                color: [
                    bg_linear.red,
                    bg_linear.green,
                    bg_linear.blue,
                    bg_linear.alpha,
                ],
                z_index: 0.0,
                corner_radius: line_corner_radius,
                skew: 0.0,
                _padding: 0.0,
            });
        }

        if line.runs.is_empty() {
            // Plain text — use layout default fg
            let color_linear = layout.default_fg.to_linear();
            let color_arr = [
                color_linear.red,
                color_linear.green,
                color_linear.blue,
                color_linear.alpha,
            ];
            let mut x = 0.0_f32;
            for ch in line.text.chars() {
                if ch == '\n' || ch == '\r' {
                    continue;
                }
                if ch == '\t' {
                    x += char_width * 4.0;
                    continue;
                }
                let key = GlyphKey::new(ch, font_size);
                if let Some(info) =
                    atlas.get_or_insert(key, || GlyphRasterizer::rasterize(ch, font_size))
                {
                    let screen_x = line_x + x + info.offset.x;
                    let screen_y = base_y - info.offset.y;
                    text_instances.push(GlyphInstance {
                        position: Vec2::new(
                            viewport_world_left + screen_x,
                            viewport_world_top - screen_y - info.size.y,
                        ),
                        uv_min: info.uv_min,
                        uv_max: info.uv_max,
                        size: info.size,
                        color: color_arr,
                        z_index: 0.0,
                        corner_radius: 0.0,
                        skew: 0.0,
                        _padding: 0.0,
                    });
                }
                x += char_width;
            }
        } else {
            // Styled runs — slice line.text by byte_range
            for run in &line.runs {
                let Some(run_text) = line.text.get(run.byte_range.clone()) else {
                    continue;
                };

                // Per-run background (skip if it duplicates the line bg)
                if let Some(bg) = run.bg {
                    if line.line_bg != Some(bg) {
                        let seg_bg_linear = bg.to_linear();
                        // Width: use char_width since runs are pre-shaped against the layout's
                        // metrics; W1 will replace this with the run's own shaped advance.
                        let visible_chars = run_text
                            .chars()
                            .filter(|c| *c != '\n' && *c != '\r')
                            .count();
                        let seg_width = visible_chars as f32 * char_width;
                        let seg_x = run_byte_to_x(line, run.byte_range.start, char_width);
                        let seg_world_x = viewport_world_left + line_x + seg_x;
                        let seg_world_y = viewport_world_top - line.y_top - line_height * 0.5;
                        below_instances.push(GlyphInstance {
                            position: Vec2::new(seg_world_x, seg_world_y),
                            uv_min: atlas.solid_uv.uv_min,
                            uv_max: atlas.solid_uv.uv_max,
                            size: Vec2::new(seg_width, line_height),
                            color: [
                                seg_bg_linear.red,
                                seg_bg_linear.green,
                                seg_bg_linear.blue,
                                seg_bg_linear.alpha,
                            ],
                            z_index: 0.0,
                            corner_radius: run.corner_radius,
                            skew: 0.0,
                            _padding: 0.0,
                        });
                    }
                }

                let color_linear = run.fg.to_linear();
                let color_arr = [
                    color_linear.red,
                    color_linear.green,
                    color_linear.blue,
                    color_linear.alpha,
                ];
                let seg_font_size = if run.font_scale > 0.0 {
                    font_size * run.font_scale
                } else {
                    font_size
                };
                let seg_char_width = if run.font_scale > 0.0 {
                    char_width * run.font_scale
                } else {
                    char_width
                };

                // Where this run starts within the line, in pixels.
                let mut x = run_byte_to_x(line, run.byte_range.start, char_width);
                for ch in run_text.chars() {
                    if ch == '\n' || ch == '\r' {
                        continue;
                    }
                    if ch == '\t' {
                        x += seg_char_width * 4.0;
                        continue;
                    }
                    let key = GlyphKey::new(ch, seg_font_size);
                    if let Some(info) =
                        atlas.get_or_insert(key, || GlyphRasterizer::rasterize(ch, seg_font_size))
                    {
                        let screen_x = line_x + x + info.offset.x;
                        let screen_y = base_y - info.offset.y;
                        text_instances.push(GlyphInstance {
                            position: Vec2::new(
                                viewport_world_left + screen_x,
                                viewport_world_top - screen_y - info.size.y,
                            ),
                            uv_min: info.uv_min,
                            uv_max: info.uv_max,
                            size: info.size,
                            color: color_arr,
                            z_index: 0.0,
                            corner_radius: 0.0,
                            skew: run.skew,
                            _padding: 0.0,
                        });
                    }
                    x += seg_char_width;
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

fn run_byte_to_x(line: &super::snapshot::ShapedLine, byte: usize, char_width: f32) -> f32 {
    // Pixel offset of `byte` within `line.text` under the monospace assumption.
    // W1 will replace this with shaped per-glyph advances.
    let prefix = line.text.get(..byte).unwrap_or("");
    let mut x = 0.0;
    for ch in prefix.chars() {
        if ch == '\t' {
            x += char_width * 4.0;
        } else if ch != '\n' && ch != '\r' {
            x += char_width;
        }
    }
    x
}

fn push_overlay_quad(
    rect: &super::overlay::RectOverlay,
    layout: &DisplayLayout,
    world_left: f32,
    world_top: f32,
    line_start_x: f32,
    solid_uv: crate::gpu_text::GlyphInfo,
    out: &mut Vec<GlyphInstance>,
) {
    let Some(line) = layout
        .lines
        .iter()
        .find(|l| l.display_row == rect.display_row)
    else {
        return;
    };
    let line_height = layout.line_height;
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


    let color_linear = rect.color.to_linear();
    out.push(GlyphInstance {
        position: Vec2::new(world_x, world_y),
        uv_min: solid_uv.uv_min,
        uv_max: solid_uv.uv_max,
        size: Vec2::new(width, height),
        color: [
            color_linear.red,
            color_linear.green,
            color_linear.blue,
            color_linear.alpha,
        ],
        z_index: 0.0,
        corner_radius: rect.corner_radius,
        skew: 0.0,
        _padding: 0.0,
    });
}
