//! Instanced GPU text rendering - alternative to per-line mesh rendering
//!
//! This module provides a high-performance instanced rendering approach
//! where all visible glyphs are rendered in a single draw call.

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use crate::settings::*;
use crate::types::*;
use crate::gpu_text::GlyphAtlas;
use super::{SyntaxResource, HighlightCache};

/// Marker component for GPU text batch entity
#[derive(Component)]
pub struct GpuTextBatch {
    /// The scroll offset when this batch was built
    pub built_at_scroll: f32,
    pub built_at_horizontal_scroll: f32,
    /// The visible line range when built
    pub first_line: usize,
    pub last_line: usize,
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
    #[cfg(feature = "folding")]
    fold_state: Res<FoldState>,
    mut atlas: ResMut<GlyphAtlas>,
    mut images: ResMut<Assets<Image>>,
    batch_query: Query<(Entity, &GpuTextBatch)>,
    mut syntax: ResMut<SyntaxResource>,
    mut highlight_cache: ResMut<HighlightCache>,
    time: Res<Time>,
) {
    if !state.needs_update {
        return;
    }

    #[cfg(not(feature = "folding"))]
    let fold_state = crate::types::FoldState::default();

    let font_size = font.size;
    let line_height = font.line_height;
    let total_buffer_lines = state.line_count();

    // Calculate visible range
    let buffer = line_height * performance.viewport_buffer_lines as f32;
    let scroll_dist = state.scroll_offset.abs();
    let start_pixels = scroll_dist - ui_settings.margin_top - buffer;
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

        // Calculate base Y position
        let baseline_offset = font_size * 0.32;
        let base_y = ui_settings.margin_top + state.scroll_offset
            + (current_display_row as f32 * line_height) + baseline_offset;

        // Get line text
        let line_text: String = if buffer_line < state.rope.len_lines() {
            let rope_line = state.rope.line(buffer_line);
            rope_line.chars().filter(|&ch| ch != '\n' && ch != '\r').collect()
        } else {
            String::new()
        };

        if line_text.is_empty() {
            current_display_row += 1;
            continue;
        }

        // Use cosmic_text to layout the line
        let positioned_glyphs = atlas.layout_line(&line_text, font_size);

        // Base X position
        let base_x = ui_settings.code_margin_left - state.horizontal_scroll_offset;

        // Process each glyph
        for positioned_glyph in positioned_glyphs.iter() {
            // Rasterize glyph
            let (cache_key, _, _) = cosmic_text::CacheKey::new(
                positioned_glyph.font_id,
                positioned_glyph.glyph_id,
                font_size * 2.0,
                (0.0, 0.0),
                cosmic_text::CacheKeyFlags::empty(),
            );

            let (glyph_info, placement) = match atlas.get_or_rasterize_glyph(cache_key) {
                Some(info) => info,
                None => continue,
            };

            // Calculate position
            let screen_x = base_x + positioned_glyph.x + placement.left;
            let screen_y = base_y + positioned_glyph.y - placement.top;

            // Convert to Bevy world coords
            let world_x = screen_x - viewport.width as f32 / 2.0 + viewport.offset_x;
            let world_y = viewport.height as f32 / 2.0 - screen_y - glyph_info.size.y;

            let color = theme.foreground;
            let color_linear = color.to_linear();

            let instance = GlyphInstance {
                position: Vec2::new(world_x, world_y),
                uv_min: glyph_info.uv_min,
                uv_max: glyph_info.uv_max,
                size: glyph_info.size,
                color: [color_linear.red, color_linear.green, color_linear.blue, color_linear.alpha],
            };

            instances.push(instance);
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
