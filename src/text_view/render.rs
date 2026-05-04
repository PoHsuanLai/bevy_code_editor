//! Generic text view rendering — produces GlyphInstance batches from TextViewState.
//!
//! This module contains the core rendering logic extracted from the editor's
//! `update_gpu_text_instanced` system. It knows nothing about cursors, selections,
//! syntax highlighting, or any other editor concept — it simply renders styled text
//! lines into a viewport.

use bevy::prelude::*;

use crate::gpu_text::{GlyphAtlas, GlyphKey, GlyphRasterizer};
use crate::settings::{FontSettings, PerformanceSettings};
use crate::types::{FoldState, LineSegment};

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

/// Render styled text from a TextViewState into glyph instances.
///
/// This is the generic rendering core. It:
/// 1. Calculates visible line range from scroll + viewport
/// 2. Reads pre-computed styled lines (or falls back to plain foreground text)
/// 3. Rasterizes glyphs via the atlas
/// 4. Produces positioned GlyphInstance data in world coordinates
///
/// Returns `(instances, budget_exceeded)`.
pub fn render_text_view(
    state: &TextViewState,
    viewport: &TextViewViewport,
    fold_state: Option<&FoldState>,
    font: &FontSettings,
    performance: &PerformanceSettings,
    foreground_color: Color,
    atlas: &mut GlyphAtlas,
    content_start_x: f32,
) -> Vec<GlyphInstance> {
    let font_size = font.size;
    let line_height = font.line_height;
    let char_width = font.char_width;
    let total_buffer_lines = state.line_count();

    let default_fold = FoldState::default();
    let fold = fold_state.unwrap_or(&default_fold);

    // Calculate visible range
    let buffer = line_height * performance.viewport_buffer_lines as f32;
    let scroll_dist = state.scroll_offset.abs();
    let start_pixels = scroll_dist - viewport.text_area_top - buffer;
    let first_visible_display_row = (start_pixels / line_height).floor().max(0.0) as usize;
    let visible_count = ((viewport.height as f32 + buffer * 2.0) / line_height).ceil() as usize;
    let last_visible_display_row = first_visible_display_row + visible_count;

    // Pre-allocate instances — backgrounds are collected separately and prepended
    let estimated_chars_per_line = 80;
    let estimated_capacity = visible_count * estimated_chars_per_line;
    let mut text_instances: Vec<GlyphInstance> = Vec::with_capacity(estimated_capacity);
    let mut bg_instances: Vec<GlyphInstance> = Vec::new();

    // Calculate start buffer line accounting for folding
    let has_folding = !fold.regions.is_empty();
    let (start_buffer_line, mut current_display_row) = if has_folding {
        let mut display_row = 0;
        let mut buffer_line = 0;
        while buffer_line < total_buffer_lines && display_row < first_visible_display_row {
            if !fold.is_line_hidden(buffer_line) {
                display_row += 1;
            }
            buffer_line += 1;
        }
        (buffer_line, display_row)
    } else {
        let start = first_visible_display_row.min(total_buffer_lines);
        (start, start)
    };

    // Base X position
    let line_start_x = content_start_x - state.horizontal_scroll_offset;

    // Render visible lines
    for buffer_line in start_buffer_line..total_buffer_lines {
        if fold.is_line_hidden(buffer_line) {
            continue;
        }

        if current_display_row > last_visible_display_row {
            break;
        }

        // Calculate base Y position
        let baseline_offset = font_size * 0.32;
        let base_y = viewport.text_area_top
            + state.scroll_offset
            + (current_display_row as f32 * line_height)
            + baseline_offset;

        // Get line text
        let rope_line = state.rope.line(buffer_line);
        let line_string = rope_line.to_string();

        // Get styled segments (pre-computed) or fall back to plain text
        let segments: &[LineSegment] = if let Some(Some(segs)) = state.styled_lines.get(buffer_line)
        {
            segs
        } else {
            &[]
        };

        // Per-line X offset (for right-alignment, indentation, etc.)
        let line_x_extra = state
            .line_x_offsets
            .get(buffer_line)
            .copied()
            .unwrap_or(0.0);

        // Current X offset relative to line start
        let mut current_x_offset: f32 = line_x_extra;

        if segments.is_empty() {
            // Plain text fallback — use foreground color
            let color_linear = foreground_color.to_linear();
            let color_arr = [
                color_linear.red,
                color_linear.green,
                color_linear.blue,
                color_linear.alpha,
            ];

            for ch in line_string.chars() {
                if ch == '\n' || ch == '\r' {
                    continue;
                }
                if ch == '\t' {
                    current_x_offset += char_width * 4.0;
                    continue;
                }

                let key = GlyphKey::new(ch, font_size);
                if let Some(info) =
                    atlas.get_or_insert(key, || GlyphRasterizer::rasterize(ch, font_size))
                {
                    let screen_x = line_start_x + current_x_offset + info.offset.x;
                    let screen_y = base_y - info.offset.y;
                    let world_x = viewport.world_left() + screen_x;
                    let world_y = viewport.world_top() - screen_y - info.size.y;

                    text_instances.push(GlyphInstance {
                        position: Vec2::new(world_x, world_y),
                        uv_min: info.uv_min,
                        uv_max: info.uv_max,
                        size: info.size,
                        color: color_arr,
                        z_index: 0.0,
                        corner_radius: 0.0,
                        skew: 0.0,
                        _padding: 0.0,
                    });
                    current_x_offset += char_width;
                } else {
                    current_x_offset += char_width;
                }
            }
        } else {
            // Styled segments — check for line-level background
            let line_bg = segments.iter().find_map(|s| s.background);
            let line_corner_radius = segments
                .iter()
                .find_map(|s| {
                    if s.background.is_some() && s.corner_radius > 0.0 {
                        Some(s.corner_radius)
                    } else {
                        None
                    }
                })
                .unwrap_or(0.0);
            if let Some(bg_color) = line_bg {
                // Emit a background quad with left/right margins.
                // For lines with X offset (right-aligned), extend the background with
                // padding on the left side of the text.
                let bg_linear = bg_color.to_linear();
                let margin = content_start_x;
                let bg_pad = if line_x_extra > 0.0 {
                    char_width * 1.5
                } else {
                    0.0
                };
                let bg_x_start = (line_x_extra - bg_pad).max(0.0);
                let bg_world_x = viewport.world_left() + margin + bg_x_start;
                let bg_width = viewport.width as f32 - margin * 2.0 - bg_x_start;
                let bg_world_y =
                    viewport.world_top() - (base_y - baseline_offset) - line_height * 0.5;
                bg_instances.push(GlyphInstance {
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

            for segment in segments {
                // Per-segment background (for inline code etc.) when different from line bg
                if let Some(seg_bg) = segment.background {
                    if line_bg != Some(seg_bg) {
                        let seg_bg_linear = seg_bg.to_linear();
                        let seg_width = segment
                            .text
                            .chars()
                            .filter(|c| *c != '\n' && *c != '\r')
                            .count() as f32
                            * char_width;
                        let seg_world_x = viewport.world_left() + line_start_x + current_x_offset;
                        let seg_world_y =
                            viewport.world_top() - (base_y - baseline_offset) - line_height * 0.5;
                        bg_instances.push(GlyphInstance {
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
                            corner_radius: segment.corner_radius,
                            skew: 0.0,
                            _padding: 0.0,
                        });
                    }
                }

                let color_linear = segment.color.to_linear();
                let color_arr = [
                    color_linear.red,
                    color_linear.green,
                    color_linear.blue,
                    color_linear.alpha,
                ];

                // Per-segment font size: font_scale > 0 overrides the default
                let seg_font_size = if segment.font_scale > 0.0 {
                    font_size * segment.font_scale
                } else {
                    font_size
                };
                let seg_char_width = if segment.font_scale > 0.0 {
                    char_width * segment.font_scale
                } else {
                    char_width
                };

                for ch in segment.text.chars() {
                    if ch == '\n' || ch == '\r' {
                        continue;
                    }
                    if ch == '\t' {
                        current_x_offset += seg_char_width * 4.0;
                        continue;
                    }

                    let key = GlyphKey::new(ch, seg_font_size);
                    if let Some(info) =
                        atlas.get_or_insert(key, || GlyphRasterizer::rasterize(ch, seg_font_size))
                    {
                        let screen_x = line_start_x + current_x_offset + info.offset.x;
                        let screen_y = base_y - info.offset.y;
                        let world_x = viewport.world_left() + screen_x;
                        let world_y = viewport.world_top() - screen_y - info.size.y;

                        text_instances.push(GlyphInstance {
                            position: Vec2::new(world_x, world_y),
                            uv_min: info.uv_min,
                            uv_max: info.uv_max,
                            size: info.size,
                            color: color_arr,
                            z_index: 0.0,
                            corner_radius: 0.0,
                            skew: segment.skew,
                            _padding: 0.0,
                        });
                        current_x_offset += seg_char_width;
                    } else {
                        current_x_offset += seg_char_width;
                    }
                }
            }
        }

        current_display_row += 1;
    }

    // Backgrounds first (painter's order), then text on top
    bg_instances.extend(text_instances);
    bg_instances
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

    // `line.y_top` is the row-top in screen-Y (already includes scroll + viewport.text_area_top
    // and excludes baseline_offset). `base_y` re-adds baseline_offset to get the glyph baseline.
    // base_y = y_top + baseline_offset.

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
        let base_y = line.y_top;
        let line_x = line_start_x + line.x_offset;

        // Line background (full-width quad)
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
            let bg_world_y =
                viewport_world_top - (base_y - baseline_offset) - line_height * 0.5;
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
                        let seg_world_y =
                            viewport_world_top - (base_y - baseline_offset) - line_height * 0.5;
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
    let base_y = line.y_top;
    let x0 = rect.x_range.start.max(0.0);
    let x1 = if rect.x_range.end >= f32::MAX / 2.0 {
        // Full-line: extend to the viewport's right edge from `line_start_x + line.x_offset`.
        // The layout doesn't carry viewport width here, so callers using f32::MAX get a sentinel.
        x0 + 1.0
    } else {
        rect.x_range.end
    };
    let width = (x1 - x0).max(1.0);
    let world_x = world_left + line_start_x + line.x_offset + x0;
    let world_y = world_top - (base_y - baseline_offset) - line_height * 0.5;
    let color_linear = rect.color.to_linear();
    out.push(GlyphInstance {
        position: Vec2::new(world_x, world_y),
        uv_min: solid_uv.uv_min,
        uv_max: solid_uv.uv_max,
        size: Vec2::new(width, line_height),
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
