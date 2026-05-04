//! GPU-accelerated line numbers rendering
//!
//! Uses the same instanced rendering pipeline as the main code text for visual consistency.

use super::editor_ui_plugin::EditorRenderConfig;
use bevy_text_engine::gpu::{GlyphAtlas, GlyphKey, GlyphRasterizer};
use crate::settings::*;
use crate::text_view::render::{GlyphBatchComponent, GlyphInstance};
use crate::text_view::{TextViewState, TextViewViewport};
use crate::types::*;
use bevy::prelude::*;

/// Marker component for the GPU line numbers batch entity
#[derive(Component)]
pub struct GpuLineNumbersBatch {
    /// Editor entity this batch belongs to
    pub editor: Entity,
    /// Content version when this batch was built
    pub built_at_version: u64,
    /// Scroll offset when this batch was built
    pub built_at_scroll: f32,
    /// Viewport dimensions when built
    pub built_at_width: u32,
    pub built_at_height: u32,
}

/// GPU-accelerated line numbers rendering system
pub(crate) fn update_gpu_line_numbers(
    mut commands: Commands,
    editor_query: Query<
        (
            Entity,
            &CodeEditorState,
            &CursorState,
            &TextViewState,
            &TextViewViewport,
            Ref<FoldState>,
        ),
        With<CodeEditor>,
    >,
    font: Res<FontSettings>,
    theme: Res<ThemeSettings>,
    ui: Res<UiSettings>,
    performance: Res<PerformanceSettings>,
    mut atlas: ResMut<GlyphAtlas>,
    render_config: Res<EditorRenderConfig>,
    mut images: ResMut<Assets<Image>>,
    batch_query: Query<(Entity, &GpuLineNumbersBatch)>,
) {
    // Hide if line numbers are disabled
    if !ui.show_line_numbers {
        for (entity, _) in batch_query.iter() {
            commands.entity(entity).insert(Visibility::Hidden);
        }
        return;
    }

    for (editor_entity, _state, cursor, tv, viewport, fold_state) in editor_query.iter() {
    // Check if we need to update
    let fold_changed = fold_state.is_changed();

    let existing_batch_for_editor = batch_query
        .iter()
        .find(|(_, b)| b.editor == editor_entity);

    if !fold_changed {
        // Check if existing batch is still valid
        if let Some((entity, batch)) = existing_batch_for_editor {
            let scroll_changed = (batch.built_at_scroll - tv.scroll_offset).abs() > 0.01;
            let viewport_changed =
                batch.built_at_width != viewport.width || batch.built_at_height != viewport.height;

            if !scroll_changed && !viewport_changed && batch.built_at_version == tv.content_version
            {
                commands.entity(entity).insert(Visibility::Visible);
                continue;
            }
        }
    }

    let line_height = font.line_height;
    let font_size = font.size;
    let _viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;

    // Collect cursor lines for highlighting active line numbers
    let cursor_lines: std::collections::HashSet<usize> = cursor
        .cursors
        .iter()
        .map(|c| {
            let pos = c.position.min(tv.rope.len_chars());
            tv.rope.char_to_line(pos)
        })
        .collect();

    // Calculate visible line range
    let buffer_lines = performance.viewport_buffer_lines as f32;
    let viewport_top = -tv.scroll_offset - line_height * buffer_lines;
    let viewport_bottom = viewport_top + viewport_height + line_height * buffer_lines * 2.0;

    let first_visible_display_row = ((viewport_top - viewport.text_area_top) / line_height)
        .floor()
        .max(0.0) as usize;
    let last_visible_display_row =
        ((viewport_bottom - viewport.text_area_top) / line_height).ceil() as usize;

    let total_buffer_lines = tv.line_count();
    let has_folding = !fold_state.regions.is_empty();

    // Calculate starting buffer line and display row
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

    // Calculate gutter center X position (camera-relative, not world coords)
    // Camera is at viewport center, so gutter is at -viewport_width/2 + gutter_width/2
    let gutter_center_x = viewport.world_left() + viewport.gutter_width / 2.0;

    // Pre-allocate instances
    let estimated_capacity = (last_visible_display_row - first_visible_display_row + 2) * 4;
    let mut instances: Vec<GlyphInstance> = Vec::with_capacity(estimated_capacity);

    // Iterate over visible buffer lines
    for buffer_line in start_buffer_line..total_buffer_lines {
        if fold_state.is_line_hidden(buffer_line) {
            continue;
        }

        if current_display_row > last_visible_display_row {
            break;
        }

        // Calculate base Y position with baseline offset to match main text
        let baseline_offset = font_size * 0.32;
        let base_y = viewport.text_area_top
            + tv.scroll_offset
            + (current_display_row as f32 * line_height)
            + baseline_offset;

        // Line number text (1-indexed)
        let line_number_text = (buffer_line + 1).to_string();

        // Use active color for cursor lines
        let line_color = if cursor_lines.contains(&buffer_line) {
            theme.line_numbers_active
        } else {
            theme.line_numbers
        };
        let color_linear = line_color.to_linear();
        let color_arr = [
            color_linear.red,
            color_linear.green,
            color_linear.blue,
            color_linear.alpha,
        ];

        // Calculate text width for right-alignment in gutter
        // Use exact metrics if available, otherwise approximation
        let mut estimated_width = 0.0;
        for ch in line_number_text.chars() {
            if let Some(w) = atlas.measure_char_width(ch, font_size) {
                estimated_width += w;
            } else {
                estimated_width += font.char_width;
            }
        }

        // Right-align: start X so that text ends near the right edge of gutter (with padding)
        let right_padding = 8.0;
        let start_x =
            gutter_center_x + viewport.gutter_width / 2.0 - right_padding - estimated_width;

        let mut x = start_x;

        // Render each character
        for ch in line_number_text.chars() {
            let key = GlyphKey::new(ch, font_size);
            if let Some(info) =
                atlas.get_or_insert(key, || GlyphRasterizer::rasterize(ch, font_size))
            {
                // Calculate screen position (same logic as main text)
                let screen_y = base_y - info.offset.y;

                // Convert to camera-relative world coordinates
                // Camera is at viewport center, entities positioned relative to camera
                let world_x = x + info.offset.x;
                let world_y = viewport.world_top() - screen_y - info.size.y;

                let instance = GlyphInstance {
                    position: Vec2::new(world_x, world_y),
                    uv_min: info.uv_min,
                    uv_max: info.uv_max,
                    size: info.size,
                    color: color_arr,
                    z_index: 0.0, // Line numbers at same level as main text
                    corner_radius: 0.0,
                    skew: 0.0,
                    _padding: 0.0,
                };

                instances.push(instance);
                x += info.advance;
            } else {
                x += font_size * 0.6;
            }
        }

        current_display_row += 1;
    }

    // Update atlas texture
    atlas.update_texture(&mut images);

    if instances.is_empty() {
        if let Some((entity, _)) = batch_query
            .iter()
            .find(|(_, b)| b.editor == editor_entity)
        {
            commands.entity(entity).insert(Visibility::Hidden);
        }
        continue;
    }

    // Update or create batch entity for this editor
    if let Some((entity, _)) = batch_query
        .iter()
        .find(|(_, b)| b.editor == editor_entity)
    {
        commands
            .entity(entity)
            .insert(GlyphBatchComponent {
                instances,
                atlas_texture: atlas.texture.clone(),
                render_layer: render_config.render_layers.as_ref().and_then(|layers| {
                    (0u8..=31).find(|&i| {
                        layers.intersects(&bevy_camera::visibility::RenderLayers::layer(i as usize))
                    })
                }),
            })
            .insert(GpuLineNumbersBatch {
                editor: editor_entity,
                built_at_version: tv.content_version,
                built_at_scroll: tv.scroll_offset,
                built_at_width: viewport.width,
                built_at_height: viewport.height,
            })
            .insert(Visibility::Visible);
    } else {
        let mut entity_cmd = commands.spawn((
            GlyphBatchComponent {
                instances,
                atlas_texture: atlas.texture.clone(),
                render_layer: render_config.render_layers.as_ref().and_then(|layers| {
                    (0u8..=31).find(|&i| {
                        layers.intersects(&bevy_camera::visibility::RenderLayers::layer(i as usize))
                    })
                }),
            },
            Transform::default(),
            GlobalTransform::default(),
            GpuLineNumbersBatch {
                editor: editor_entity,
                built_at_version: tv.content_version,
                built_at_scroll: tv.scroll_offset,
                built_at_width: viewport.width,
                built_at_height: viewport.height,
            },
            Name::new("GpuLineNumbersBatch"),
            Visibility::Visible,
            InheritedVisibility::default(),
            ViewVisibility::default(),
        ));

        if let Some(ref layers) = render_config.render_layers {
            entity_cmd.insert(layers.clone());
        }
    }
    }
}
