//! Instanced GPU text rendering - alternative to per-line mesh rendering
//!
//! This module provides a high-performance instanced rendering approach
//! where all visible glyphs are rendered in a single draw call.

use super::{HighlightCache, SyntaxResource};
use crate::gpu_text::{GlyphAtlas, GlyphKey, GlyphRasterizer};
use crate::settings::*;
use crate::types::*;
use bevy::prelude::*;

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

/// Component containing batch of glyph instances
#[derive(Component, Clone)]
pub struct GlyphBatchComponent {
    pub instances: Vec<GlyphInstance>,
    pub atlas_texture: Handle<Image>,
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
    // Use the measured char_width for grid alignment
    let char_width = font.char_width;
    let total_buffer_lines = state.line_count();

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

    // Render visible lines
    for buffer_line in start_buffer_line..total_buffer_lines {
        if fold_state.is_line_hidden(buffer_line) {
            continue;
        }

        if current_display_row > last_visible_display_row {
            break;
        }

        // Calculate base Y position (use viewport.text_area_top to match line numbers)
        let baseline_offset = font_size * 0.32;
        let base_y = viewport.text_area_top
            + state.scroll_offset
            + (current_display_row as f32 * line_height)
            + baseline_offset;

        // Get line text from rope
        let rope_line = state.rope.line(buffer_line);
        let line_string = rope_line.to_string();

        // Get syntax highlighting segments
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
            // Fallback: one segment for the whole line
            // Only used if syntax highlighting returns nothing (e.g. error or initial state)
            &vec![]
        };

        // Base X position (starting point for this line)
        let line_start_x = viewport
            .text_area_left
            .max(viewport.gutter_width + ui_settings.code_margin_left)
            - state.horizontal_scroll_offset;

        // Current X offset relative to line start (accumulated by char width)
        let mut current_x_offset = 0.0;

        // Process segments
        if segments.is_empty() {
            // Plain text fallback
            let color_linear = theme.foreground.to_linear();
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
                    // Strict Grid Positioning: use current_x_offset instead of info.advance
                    let screen_x = line_start_x + current_x_offset + info.offset.x;
                    let screen_y = base_y - info.offset.y;

                    let world_x = viewport.world_left() + screen_x;
                    let world_y = viewport.world_top() - screen_y - info.size.y;

                    instances.push(GlyphInstance {
                        position: Vec2::new(world_x, world_y),
                        uv_min: info.uv_min,
                        uv_max: info.uv_max,
                        size: info.size,
                        color: color_arr,
                        z_index: 0.0, // Main editor text
                        _padding: [0.0; 3],
                    });

                    // Advance by fixed char_width, NOT info.advance
                    current_x_offset += char_width;
                } else {
                    current_x_offset += char_width;
                }
            }
        } else {
            // Syntax highlighted segments
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
                        // Strict Grid Positioning
                        let screen_x = line_start_x + current_x_offset + info.offset.x;
                        let screen_y = base_y - info.offset.y;

                        let world_x = viewport.world_left() + screen_x;
                        let world_y = viewport.world_top() - screen_y - info.size.y;

                        instances.push(GlyphInstance {
                            position: Vec2::new(world_x, world_y),
                            uv_min: info.uv_min,
                            uv_max: info.uv_max,
                            size: info.size,
                            color: color_arr,
                            z_index: 0.0, // Main editor text
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

    // Update atlas texture
    atlas.update_texture(&mut images);

    // Update or create batch entity
    if instances.is_empty() {
        for (entity, _) in batch_query.iter() {
            commands.entity(entity).insert(Visibility::Hidden);
        }
    } else {
        let existing_batches: Vec<Entity> = batch_query.iter().map(|(e, _)| e).collect();

        if let Some(&first_entity) = existing_batches.first() {
            commands.entity(first_entity).insert(GlyphBatchComponent {
                instances,
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
                    instances,
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
