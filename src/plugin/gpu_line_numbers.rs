//! GPU-accelerated line numbers rendering
//!
//! Uses the same rendering pipeline as the main code text for visual consistency.

use super::editor_ui_plugin::EditorRenderConfig;
use crate::gpu_text::{GlyphAtlas, GlyphKey, GlyphRasterizer, TextMaterial, TextRenderState};
use crate::settings::*;
use crate::types::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::sprite_render::MeshMaterial2d;

/// Marker component for the GPU line numbers mesh entity
#[derive(Component)]
pub struct GpuLineNumbersMesh {
    /// Content version when this mesh was built
    pub built_at_version: u64,
    /// Scroll offset when this mesh was built
    pub built_at_scroll: f32,
    /// Viewport dimensions when built
    pub built_at_width: u32,
    pub built_at_height: u32,
}

/// GPU-accelerated line numbers rendering system
pub(crate) fn update_gpu_line_numbers(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    font: Res<FontSettings>,
    theme: Res<ThemeSettings>,
    ui: Res<UiSettings>,
    performance: Res<PerformanceSettings>,
    viewport: Res<ViewportDimensions>,
    fold_state: Res<FoldState>,
    mut atlas: ResMut<GlyphAtlas>,
    render_state: Res<TextRenderState>,
    render_config: Res<EditorRenderConfig>,
    mut materials: ResMut<Assets<TextMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mesh_query: Query<(Entity, &GpuLineNumbersMesh, &bevy::mesh::Mesh2d)>,
) {
    // Hide if line numbers are disabled
    if !ui.show_line_numbers {
        for (entity, _, _) in mesh_query.iter() {
            commands.entity(entity).insert(Visibility::Hidden);
        }
        return;
    }

    // Check if we need to update
    if !state.needs_update && !state.needs_scroll_update && !state.is_changed() && !fold_state.is_changed() {
        // Check if existing mesh is still valid
        if let Some((entity, line_mesh, _)) = mesh_query.iter().next() {
            let scroll_changed = (line_mesh.built_at_scroll - state.scroll_offset).abs() > 0.01;
            let viewport_changed = line_mesh.built_at_width != viewport.width
                || line_mesh.built_at_height != viewport.height;

            if !scroll_changed && !viewport_changed && line_mesh.built_at_version == state.content_version {
                commands.entity(entity).insert(Visibility::Visible);
                return;
            }
        }
    }

    let line_height = font.line_height;
    let font_size = font.size;
    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;

    // Collect cursor lines for highlighting active line numbers
    let cursor_lines: std::collections::HashSet<usize> = state
        .cursors
        .iter()
        .map(|c| {
            let pos = c.position.min(state.rope.len_chars());
            state.rope.char_to_line(pos)
        })
        .collect();

    // Calculate visible line range
    let buffer_lines = performance.viewport_buffer_lines as f32;
    let viewport_top = -state.scroll_offset - line_height * buffer_lines;
    let viewport_bottom = viewport_top + viewport_height + line_height * buffer_lines * 2.0;

    let first_visible_display_row = ((viewport_top - viewport.text_area_top) / line_height)
        .floor()
        .max(0.0) as usize;
    let last_visible_display_row =
        ((viewport_bottom - viewport.text_area_top) / line_height).ceil() as usize;

    let total_buffer_lines = state.line_count();
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

    // Build mesh data
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut vertex_count: u32 = 0;

    // Calculate gutter center X position in world coordinates
    let gutter_center_x = -viewport_width / 2.0 + viewport.gutter_width / 2.0;

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
            + state.scroll_offset
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
        let color_arr = line_color.to_linear().to_f32_array();

        // Calculate text width for right-alignment in gutter
        let char_count = line_number_text.len();
        let estimated_width = char_count as f32 * font.char_width;

        // Right-align: start X so that text ends near the right edge of gutter (with padding)
        let right_padding = 8.0;
        let start_x = gutter_center_x + viewport.gutter_width / 2.0 - right_padding - estimated_width;

        let mut x = start_x;

        // Render each character
        for ch in line_number_text.chars() {
            let key = GlyphKey::new(ch, font_size);
            if let Some(info) = atlas.get_or_insert(key, || GlyphRasterizer::rasterize(ch, font_size)) {
                // Calculate screen position first (same as main text renderer)
                let screen_y = base_y - info.offset.y;
                // Convert to world coordinates
                let world_x = x + info.offset.x;
                let world_y = viewport_height / 2.0 - screen_y;

                let w = info.size.x;
                let h = info.size.y;

                // Four corners of the glyph quad
                positions.push([world_x, world_y - h, 0.0]);
                positions.push([world_x + w, world_y - h, 0.0]);
                positions.push([world_x + w, world_y, 0.0]);
                positions.push([world_x, world_y, 0.0]);

                // UV coordinates
                uvs.push([info.uv_min.x, info.uv_max.y]);
                uvs.push([info.uv_max.x, info.uv_max.y]);
                uvs.push([info.uv_max.x, info.uv_min.y]);
                uvs.push([info.uv_min.x, info.uv_min.y]);

                // Colors
                colors.push(color_arr);
                colors.push(color_arr);
                colors.push(color_arr);
                colors.push(color_arr);

                // Indices
                indices.push(vertex_count);
                indices.push(vertex_count + 1);
                indices.push(vertex_count + 2);
                indices.push(vertex_count);
                indices.push(vertex_count + 2);
                indices.push(vertex_count + 3);

                vertex_count += 4;
                x += info.advance;
            } else {
                x += font_size * 0.6;
            }
        }

        current_display_row += 1;
    }

    // Update material and atlas
    let Some(material_handle) = &render_state.material_handle else {
        return;
    };

    if let Some(material) = materials.get_mut(material_handle) {
        material.atlas_texture = atlas.texture.clone();
    }

    atlas.update_texture(&mut images);

    if positions.is_empty() {
        for (entity, _, _) in mesh_query.iter() {
            commands.entity(entity).insert(Visibility::Hidden);
        }
        return;
    }

    // Build mesh
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));

    // Update or create mesh entity
    if let Some((entity, line_mesh, _)) = mesh_query.iter().next() {
        let scroll_changed = (line_mesh.built_at_scroll - state.scroll_offset).abs() > 0.01;
        let viewport_changed = line_mesh.built_at_width != viewport.width
            || line_mesh.built_at_height != viewport.height;
        let needs_rebuild = line_mesh.built_at_version != state.content_version
            || scroll_changed
            || viewport_changed;

        if needs_rebuild {
            let new_mesh_handle = meshes.add(mesh);
            commands
                .entity(entity)
                .insert(bevy::mesh::Mesh2d(new_mesh_handle))
                .insert(GpuLineNumbersMesh {
                    built_at_version: state.content_version,
                    built_at_scroll: state.scroll_offset,
                    built_at_width: viewport.width,
                    built_at_height: viewport.height,
                })
                .insert(Visibility::Visible);
        } else {
            commands.entity(entity).insert(Visibility::Visible);
        }
    } else {
        // Create new mesh entity
        let mesh_handle = meshes.add(mesh);
        let mut entity_cmd = commands.spawn((
            bevy::mesh::Mesh2d(mesh_handle),
            MeshMaterial2d(material_handle.clone()),
            Transform::from_translation(Vec3::new(0.0, 0.0, 0.5)), // Slightly behind main text
            GpuLineNumbersMesh {
                built_at_version: state.content_version,
                built_at_scroll: state.scroll_offset,
                built_at_width: viewport.width,
                built_at_height: viewport.height,
            },
            Name::new("GpuLineNumbersMesh"),
            Visibility::Visible,
        ));
        if let Some(ref layers) = render_config.render_layers {
            entity_cmd.insert(layers.clone());
        }
    }
}
