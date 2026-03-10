//! Instanced GPU text rendering - alternative to per-line mesh rendering
//!
//! This module provides a high-performance instanced rendering approach
//! where all visible glyphs are rendered in a single draw call.
//!
//! Per-line glyph caching: each line's glyphs are cached with relative X positions
//! and colors. Only dirty lines get re-shaped. Scroll-only changes reuse all cached
//! glyphs and only recompute Y positions.

use super::{HighlightCache, SyntaxResource};
use crate::gpu_text::{GlyphAtlas, GlyphKey, GlyphRasterizer};
use crate::settings::*;
use crate::types::*;
use bevy::prelude::*;
use std::sync::Arc;

/// Marker component for GPU text batch entity
#[derive(Component)]
pub struct GpuTextBatch {
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
    pub _padding: [f32; 3], // Pad to 16-byte alignment
}

/// Component containing batch of glyph instances.
/// Uses Arc to avoid cloning thousands of glyphs during render extract phase.
#[derive(Component, Clone)]
pub struct GlyphBatchComponent {
    pub instances: Arc<Vec<GlyphInstance>>,
    pub atlas_texture: Handle<Image>,
}

/// A cached glyph for a single character, storing position relative to line start
#[derive(Clone, Copy, Debug)]
struct CachedGlyph {
    /// X offset relative to line_start_x (before horizontal scroll)
    rel_x: f32,
    /// Y offset from baseline (info.offset)
    offset_x: f32,
    offset_y: f32,
    uv_min: Vec2,
    uv_max: Vec2,
    size: Vec2,
    color: [f32; 4],
}

/// Per-line glyph cache with reusable instance buffer
#[derive(Resource)]
pub struct LineGlyphCache {
    /// Cached glyphs per buffer line. Index = buffer line number.
    lines: Vec<Option<Vec<CachedGlyph>>>,
    /// Content version when each line was last cached
    versions: Vec<u64>,
    /// Global content version counter for invalidation
    content_version: u64,
}

impl Default for LineGlyphCache {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            versions: Vec::new(),
            content_version: 0,
        }
    }
}

impl LineGlyphCache {
    fn ensure_capacity(&mut self, line_count: usize) {
        if self.lines.len() < line_count {
            self.lines.resize(line_count, None);
            self.versions.resize(line_count, 0);
        } else if self.lines.len() > line_count + 1000 {
            // Shrink if way too large
            self.lines.truncate(line_count);
            self.versions.truncate(line_count);
        }
    }

    fn invalidate_range(&mut self, range: std::ops::Range<usize>) {
        for i in range {
            if i < self.lines.len() {
                self.lines[i] = None;
            }
        }
    }

    fn invalidate_all(&mut self) {
        for line in &mut self.lines {
            *line = None;
        }
    }
}

/// Build cached glyphs for a single line (relative positions, no Y baked in)
fn build_line_glyphs(
    line_text: &str,
    segments: &[LineSegment],
    char_width: f32,
    font_size: f32,
    default_color: Color,
    atlas: &mut GlyphAtlas,
) -> Vec<CachedGlyph> {
    let mut glyphs = Vec::new();
    let mut current_x_offset: f32 = 0.0;

    if segments.is_empty() {
        let color_linear = default_color.to_linear();
        let color_arr = [
            color_linear.red,
            color_linear.green,
            color_linear.blue,
            color_linear.alpha,
        ];

        for ch in line_text.chars() {
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
                glyphs.push(CachedGlyph {
                    rel_x: current_x_offset,
                    offset_x: info.offset.x,
                    offset_y: info.offset.y,
                    uv_min: info.uv_min,
                    uv_max: info.uv_max,
                    size: info.size,
                    color: color_arr,
                });
                current_x_offset += char_width;
            } else {
                current_x_offset += char_width;
            }
        }
    } else {
        for segment in segments {
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
                    glyphs.push(CachedGlyph {
                        rel_x: current_x_offset,
                        offset_x: info.offset.x,
                        offset_y: info.offset.y,
                        uv_min: info.uv_min,
                        uv_max: info.uv_max,
                        size: info.size,
                        color: color_arr,
                    });
                    current_x_offset += char_width;
                } else {
                    current_x_offset += char_width;
                }
            }
        }
    }

    glyphs
}

/// System to update instanced GPU text display
pub(crate) fn update_gpu_text_instanced(
    mut commands: Commands,
    mut state: ResMut<CodeEditorState>,
    (font, theme, syntax_settings, ui_settings, performance): (
        Res<FontSettings>,
        Res<ThemeSettings>,
        Res<SyntaxSettings>,
        Res<UiSettings>,
        Res<PerformanceSettings>,
    ),
    viewport: Res<ViewportDimensions>,
    #[cfg(feature = "folding")] fold_state: Res<FoldState>,
    mut atlas: ResMut<GlyphAtlas>,
    mut images: ResMut<Assets<Image>>,
    batch_query: Query<(Entity, &GpuTextBatch)>,
    mut syntax: ResMut<SyntaxResource>,
    _highlight_cache: ResMut<HighlightCache>,
    mut line_cache: ResMut<LineGlyphCache>,
    time: Res<Time>,
) {
    // Check if viewport changed
    let viewport_changed = if let Some((_, batch)) = batch_query.iter().next() {
        batch.built_at_width != viewport.width || batch.built_at_height != viewport.height
    } else {
        true
    };

    // Update if content changed OR scroll changed OR viewport changed
    if !state.needs_update && !state.needs_scroll_update && !viewport_changed {
        return;
    }

    #[cfg(not(feature = "folding"))]
    let fold_state = crate::types::FoldState::default();

    let font_size = font.size;
    let line_height = font.line_height;
    let char_width = font.char_width;
    let total_buffer_lines = state.line_count();

    // Manage line cache
    line_cache.ensure_capacity(total_buffer_lines);

    // Invalidate dirty lines in cache
    if state.needs_update {
        let new_version = state.content_version;
        if new_version != line_cache.content_version {
            if let Some(ref dirty) = state.dirty_lines {
                // Only invalidate the dirty range
                let end = dirty.end.min(total_buffer_lines);
                line_cache.invalidate_range(dirty.start..end);
            } else {
                // Full invalidation (paste, undo, language change, etc.)
                line_cache.invalidate_all();
            }
            line_cache.content_version = new_version;
        }
    }

    // Calculate visible range
    let buffer = line_height * performance.viewport_buffer_lines as f32;
    let scroll_dist = state.scroll_offset.abs();
    let start_pixels = scroll_dist - viewport.text_area_top - buffer;
    let first_visible_display_row = (start_pixels / line_height).floor().max(0.0) as usize;
    let visible_count = ((viewport.height as f32 + buffer * 2.0) / line_height).ceil() as usize;
    let last_visible_display_row = first_visible_display_row + visible_count;

    // Pre-allocate instances vector
    let estimated_chars_per_line = 80;
    let estimated_capacity = visible_count * estimated_chars_per_line;
    let mut instances: Vec<GlyphInstance> = Vec::with_capacity(estimated_capacity);

    // Calculate start buffer line accounting for folding
    let has_folding = !fold_state.regions.is_empty();
    let (start_buffer_line, mut current_display_row) = if has_folding {
        let mut display_row = 0;
        let mut buffer_line = 0;
        while buffer_line < total_buffer_lines && display_row < first_visible_display_row {
            if !fold_state.is_line_hidden(buffer_line) {
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
    let line_start_x = viewport
        .text_area_left
        .max(viewport.gutter_width + ui_settings.code_margin_left)
        - state.horizontal_scroll_offset;

    // Render visible lines
    for buffer_line in start_buffer_line..total_buffer_lines {
        if fold_state.is_line_hidden(buffer_line) {
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

        // Get or build cached glyphs for this line
        let cached = if buffer_line < line_cache.lines.len() {
            &line_cache.lines[buffer_line]
        } else {
            &None
        };

        let glyphs = if let Some(cached_glyphs) = cached {
            cached_glyphs
        } else {
            // Cache miss — build glyphs for this line
            let rope_line = state.rope.line(buffer_line);
            let line_string = rope_line.to_string();

            let segments_vec = syntax.highlight_range(
                &line_string,
                buffer_line,
                buffer_line + 1,
                state.rope.line_to_byte(buffer_line),
                &syntax_settings.theme,
                theme.foreground,
            );

            let segments = if !segments_vec.is_empty() {
                &segments_vec[0]
            } else {
                &vec![]
            };

            let new_glyphs = build_line_glyphs(
                &line_string,
                segments,
                char_width,
                font_size,
                theme.foreground,
                &mut atlas,
            );

            if buffer_line < line_cache.lines.len() {
                line_cache.lines[buffer_line] = Some(new_glyphs);
                line_cache.lines[buffer_line].as_ref().unwrap()
            } else {
                // Shouldn't happen after ensure_capacity, but safety fallback
                // Just emit directly without caching
                let rope_line = state.rope.line(buffer_line);
                let line_string = rope_line.to_string();
                let segments_vec = syntax.highlight_range(
                    &line_string,
                    buffer_line,
                    buffer_line + 1,
                    state.rope.line_to_byte(buffer_line),
                    &syntax_settings.theme,
                    theme.foreground,
                );
                let segments = if !segments_vec.is_empty() {
                    &segments_vec[0]
                } else {
                    &vec![]
                };
                // Build directly into instances and continue
                let fallback = build_line_glyphs(
                    &line_string,
                    segments,
                    char_width,
                    font_size,
                    theme.foreground,
                    &mut atlas,
                );
                for g in &fallback {
                    let screen_x = line_start_x + g.rel_x + g.offset_x;
                    let screen_y = base_y - g.offset_y;
                    instances.push(GlyphInstance {
                        position: Vec2::new(
                            viewport.world_left() + screen_x,
                            viewport.world_top() - screen_y - g.size.y,
                        ),
                        uv_min: g.uv_min,
                        uv_max: g.uv_max,
                        size: g.size,
                        color: g.color,
                        z_index: 0.0,
                        _padding: [0.0; 3],
                    });
                }
                current_display_row += 1;
                continue;
            }
        };

        // Emit instances from cached glyphs with current position
        for g in glyphs {
            let screen_x = line_start_x + g.rel_x + g.offset_x;
            let screen_y = base_y - g.offset_y;

            instances.push(GlyphInstance {
                position: Vec2::new(
                    viewport.world_left() + screen_x,
                    viewport.world_top() - screen_y - g.size.y,
                ),
                uv_min: g.uv_min,
                uv_max: g.uv_max,
                size: g.size,
                color: g.color,
                z_index: 0.0,
                _padding: [0.0; 3],
            });
        }

        current_display_row += 1;
    }

    // Update atlas texture
    atlas.update_texture(&mut images);

    // Wrap in Arc for zero-copy render extract; put buffer back for reuse next frame
    let arc_instances = Arc::new(instances);

    // Update or create batch entity
    if arc_instances.is_empty() {
        for (entity, _) in batch_query.iter() {
            commands.entity(entity).insert(Visibility::Hidden);
        }
    } else {
        let existing_batches: Vec<Entity> = batch_query.iter().map(|(e, _)| e).collect();

        if let Some(&first_entity) = existing_batches.first() {
            commands.entity(first_entity).insert(GlyphBatchComponent {
                instances: arc_instances,
                atlas_texture: atlas.texture.clone(),
            });
            commands.entity(first_entity).insert(Visibility::Visible);
            commands.entity(first_entity).insert(GpuTextBatch {
                built_at_scroll: state.scroll_offset,
                built_at_horizontal_scroll: state.horizontal_scroll_offset,
                first_line: first_visible_display_row,
                last_line: last_visible_display_row,
                built_at_width: viewport.width,
                built_at_height: viewport.height,
            });

            // Despawn extras
            for &entity in &existing_batches[1..] {
                commands.entity(entity).despawn();
            }
        } else {
            commands.spawn((
                GlyphBatchComponent {
                    instances: arc_instances,
                    atlas_texture: atlas.texture.clone(),
                },
                Transform::default(),
                GlobalTransform::default(),
                GpuTextBatch {
                    built_at_scroll: state.scroll_offset,
                    built_at_horizontal_scroll: state.horizontal_scroll_offset,
                    first_line: first_visible_display_row,
                    last_line: last_visible_display_row,
                    built_at_width: viewport.width,
                    built_at_height: viewport.height,
                },
                Name::new("GpuTextBatch"),
                Visibility::Visible,
                InheritedVisibility::default(),
                ViewVisibility::default(),
            ));
        }
    }

    state.needs_update = false;
    state.last_render_time = time.elapsed_secs_f64() * 1000.0;
}
