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
    pub _padding: [f32; 3],
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

        // Current X offset relative to line start
        let mut current_x_offset: f32 = 0.0;

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
                        _padding: [0.0; 3],
                    });
                    current_x_offset += char_width;
                } else {
                    current_x_offset += char_width;
                }
            }
        } else {
            // Styled segments — check for line-level background
            let line_bg = segments.iter().find_map(|s| s.background);
            if let Some(bg_color) = line_bg {
                // Emit a background quad with left/right margins matching the text area
                let bg_linear = bg_color.to_linear();
                let margin = content_start_x;
                let bg_world_x = viewport.world_left() + margin;
                let bg_width = viewport.width as f32 - margin * 2.0;
                let bg_world_y = viewport.world_top() - (base_y - baseline_offset) - line_height * 0.5;
                bg_instances.push(GlyphInstance {
                    position: Vec2::new(bg_world_x, bg_world_y),
                    uv_min: atlas.solid_uv.uv_min,
                    uv_max: atlas.solid_uv.uv_max,
                    size: Vec2::new(bg_width, line_height),
                    color: [bg_linear.red, bg_linear.green, bg_linear.blue, bg_linear.alpha],
                    z_index: 0.0,
                    _padding: [0.0; 3],
                });
            }

            for segment in segments {
                // Per-segment background (for inline code etc.) when different from line bg
                if let Some(seg_bg) = segment.background {
                    if line_bg != Some(seg_bg) {
                        let seg_bg_linear = seg_bg.to_linear();
                        let seg_width = segment.text.chars().filter(|c| *c != '\n' && *c != '\r').count() as f32 * char_width;
                        let seg_world_x = viewport.world_left() + line_start_x + current_x_offset;
                        let seg_world_y = viewport.world_top() - (base_y - baseline_offset) - line_height * 0.5;
                        bg_instances.push(GlyphInstance {
                            position: Vec2::new(seg_world_x, seg_world_y),
                            uv_min: atlas.solid_uv.uv_min,
                            uv_max: atlas.solid_uv.uv_max,
                            size: Vec2::new(seg_width, line_height),
                            color: [seg_bg_linear.red, seg_bg_linear.green, seg_bg_linear.blue, seg_bg_linear.alpha],
                            z_index: 0.0,
                            _padding: [0.0; 3],
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

                for ch in segment.text.chars() {
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
                            _padding: [0.0; 3],
                        });
                        current_x_offset += char_width;
                    } else {
                        current_x_offset += char_width;
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
