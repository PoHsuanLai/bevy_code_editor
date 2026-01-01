//! Minimap rendering and interaction

use super::editor_ui_plugin::EditorRenderConfig;
use crate::gpu_text::{GlyphAtlas, GlyphKey, GlyphRasterizer, TextMaterial, TextRenderState};
use crate::settings::*;
use crate::types::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::sprite_render::MeshMaterial2d;

pub(crate) fn update_minimap_hover(
    windows: Query<&Window>,
    viewport: Res<ViewportDimensions>,
    minimap_settings: Res<MinimapSettings>,
    mut hover_state: ResMut<MinimapHoverState>,
) {
    if !minimap_settings.should_show(viewport.width as f32) {
        hover_state.is_hovered = false;
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };

    let Some(cursor_pos) = window.cursor_position() else {
        hover_state.is_hovered = false;
        return;
    };

    let viewport_width = viewport.width as f32;
    let minimap_width = minimap_settings.width;

    // Check if cursor is over the minimap area (accounting for edge padding)
    let is_over_minimap = if minimap_settings.show_on_right {
        cursor_pos.x >= viewport_width - minimap_width - minimap_settings.edge_padding
    } else {
        cursor_pos.x <= minimap_width + minimap_settings.edge_padding
    };

    hover_state.is_hovered = is_over_minimap;
}

/// Handle mouse clicks and drags on the minimap for click-to-scroll and drag-to-scroll
pub(crate) fn handle_minimap_mouse(
    windows: Query<&Window>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut state: ResMut<CodeEditorState>,
    font: Res<FontSettings>,
    minimap_settings: Res<MinimapSettings>,
    viewport: Res<ViewportDimensions>,
    hover_state: Res<MinimapHoverState>,
    mut drag_state: ResMut<MinimapDragState>,
    highlight_query: Query<(&Transform, &Sprite), With<MinimapViewportHighlight>>,
) {
    if !minimap_settings.should_show(viewport.width as f32) {
        drag_state.is_dragging = false;
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };

    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };

    let viewport_height = viewport.height as f32;
    let line_count = state.rope.len_lines();
    let line_height = font.line_height;

    // Minimap settings (same as in update_minimap)
    let minimap_line_height = minimap_settings.line_height;
    let total_minimap_content_height = line_count as f32 * minimap_line_height;

    // Content Y offset for centering
    let content_y_offset =
        if minimap_settings.center_when_short && total_minimap_content_height < viewport_height {
            (viewport_height - total_minimap_content_height) / 2.0
        } else {
            0.0
        };

    // Calculate minimap scroll offset (same as in update_minimap)
    let content_height = line_count as f32 * line_height;
    let max_scroll = -(content_height - viewport_height).max(0.0);
    let scroll_progress = if max_scroll < 0.0 {
        (state.scroll_offset / max_scroll).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let minimap_scroll_offset = if total_minimap_content_height > viewport_height {
        let max_minimap_scroll = total_minimap_content_height - viewport_height;
        scroll_progress * max_minimap_scroll
    } else {
        0.0
    };

    // Handle mouse button release
    if mouse_button.just_released(MouseButton::Left) {
        drag_state.is_dragging = false;
        drag_state.is_dragging_highlight = false;
    }

    // Check if cursor is over the viewport highlight
    let is_over_highlight = if let Ok((transform, sprite)) = highlight_query.single() {
        if let Some(size) = sprite.custom_size {
            // Convert cursor to world coordinates
            let cursor_world_y = cursor_pos.y - viewport_height / 2.0;

            let highlight_y = transform.translation.y;
            let highlight_half_height = size.y / 2.0;

            cursor_world_y >= highlight_y - highlight_half_height
                && cursor_world_y <= highlight_y + highlight_half_height
        } else {
            false
        }
    } else {
        false
    };

    // Handle mouse button press on minimap
    if mouse_button.just_pressed(MouseButton::Left) && hover_state.is_hovered {
        drag_state.is_dragging = true;

        // Check if clicking on the viewport highlight
        if is_over_highlight {
            drag_state.is_dragging_highlight = true;
            drag_state.drag_start_y = cursor_pos.y;
            drag_state.drag_start_scroll = state.scroll_offset;
        } else {
            drag_state.is_dragging_highlight = false;
        }
    }

    // Handle dragging the viewport highlight
    if drag_state.is_dragging
        && drag_state.is_dragging_highlight
        && mouse_button.pressed(MouseButton::Left)
    {
        // Calculate how far the mouse has moved (in screen space)
        let delta_y = cursor_pos.y - drag_state.drag_start_y;

        // Scale delta to minimap content space
        // When minimap is scrollable, we need to account for the minimap scroll ratio
        let minimap_to_content_ratio = if total_minimap_content_height > 0.0 {
            content_height / total_minimap_content_height
        } else {
            1.0
        };

        // Apply the delta to the original scroll position
        let new_scroll = drag_state.drag_start_scroll - (delta_y * minimap_to_content_ratio);

        // Clamp to valid range
        state.scroll_offset = new_scroll.clamp(max_scroll.min(0.0), 0.0);
        state.needs_scroll_update = true;
    }
    // Handle click or drag elsewhere on minimap (jump-to-position behavior)
    else if drag_state.is_dragging
        && !drag_state.is_dragging_highlight
        && ((mouse_button.just_pressed(MouseButton::Left) && hover_state.is_hovered)
            || mouse_button.pressed(MouseButton::Left))
    {
        // Convert cursor Y position to minimap content position
        // cursor_pos.y is from top of window (0 = top)
        let click_y_in_minimap = cursor_pos.y - content_y_offset + minimap_scroll_offset;

        // Calculate which line was clicked in the minimap
        let clicked_line = (click_y_in_minimap / minimap_line_height).floor() as usize;
        let clicked_line = clicked_line.min(line_count.saturating_sub(1));

        // Calculate scroll position to show this line in the center of the viewport
        let visible_lines = viewport_height / line_height;
        let target_first_line = (clicked_line as f32 - visible_lines / 2.0).max(0.0);

        // Convert line position to scroll offset
        let target_scroll = -(target_first_line * line_height);

        // Clamp to valid range
        state.scroll_offset = target_scroll.clamp(max_scroll.min(0.0), 0.0);
        state.needs_scroll_update = true;
    }
}

/// Update minimap rendering using GPU-accelerated text
pub(crate) fn update_minimap(
    mut commands: Commands,
    state: ResMut<CodeEditorState>,
    (font, theme, minimap_settings): (Res<FontSettings>, Res<ThemeSettings>, Res<MinimapSettings>),
    viewport: Res<ViewportDimensions>,
    hover_state: Res<MinimapHoverState>,
    mut atlas: ResMut<GlyphAtlas>,
    render_state: Res<TextRenderState>,
    render_config: Res<EditorRenderConfig>,
    mut materials: ResMut<Assets<TextMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mesh_query: Query<(Entity, &GpuMinimapMesh, &bevy::mesh::Mesh2d)>,
    mut syntax: ResMut<super::SyntaxResource>,
    mut bg_query: Query<
        (Entity, &mut Transform, &mut Sprite, &mut Visibility),
        (
            With<MinimapBackground>,
            Without<MinimapSlider>,
            Without<MinimapViewportHighlight>,
        ),
    >,
    mut slider_query: Query<
        (Entity, &mut Transform, &mut Sprite, &mut Visibility),
        (
            With<MinimapSlider>,
            Without<MinimapBackground>,
            Without<MinimapViewportHighlight>,
        ),
    >,
    mut highlight_query: Query<
        (Entity, &mut Transform, &mut Sprite, &mut Visibility),
        (
            With<MinimapViewportHighlight>,
            Without<MinimapBackground>,
            Without<MinimapSlider>,
        ),
    >,
) {
    let viewport_width = viewport.width as f32;

    // Hide everything if minimap is disabled or viewport is too narrow
    if !minimap_settings.should_show(viewport_width) {
        for (_, _, _, mut visibility) in bg_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        for (_, _, _, mut visibility) in slider_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        for (_, _, _, mut visibility) in highlight_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        for (entity, _, _) in mesh_query.iter() {
            commands.entity(entity).insert(Visibility::Hidden);
        }
        return;
    }
    let viewport_height = viewport.height as f32;
    let minimap_width = minimap_settings.width;
    let line_count = state.rope.len_lines();
    let line_height = font.line_height;

    // Minimap text settings - tiny font like VSCode
    let minimap_line_height = minimap_settings.line_height;
    let minimap_font_size = minimap_settings.font_size;

    // Calculate total content height in minimap (unscaled)
    let total_minimap_content_height = line_count as f32 * minimap_line_height;

    // Vertical offset for centering when content is short
    let content_y_offset =
        if minimap_settings.center_when_short && total_minimap_content_height < viewport_height {
            (viewport_height - total_minimap_content_height) / 2.0
        } else {
            0.0
        };

    let minimap_center_x = if minimap_settings.show_on_right {
        viewport_width / 2.0 - minimap_width / 2.0 - minimap_settings.edge_padding
    } else {
        -viewport_width / 2.0 + minimap_width / 2.0 + minimap_settings.edge_padding
    };

    // === BACKGROUND ===
    if let Ok((_, mut transform, mut sprite, mut visibility)) = bg_query.single_mut() {
        sprite.custom_size = Some(Vec2::new(minimap_width, viewport_height));
        transform.translation =
            Vec3::new(minimap_center_x, 0.0, minimap_settings.background_z_index);
        *visibility = Visibility::Visible;
    } else {
        let mut entity_cmd = commands.spawn((
            Sprite {
                color: theme.minimap_background,
                custom_size: Some(Vec2::new(minimap_width, viewport_height)),
                ..default()
            },
            Transform::from_translation(Vec3::new(
                minimap_center_x,
                0.0,
                minimap_settings.background_z_index,
            )),
            MinimapBackground,
            Name::new("MinimapBackground"),
            Visibility::Visible,
        ));
        if let Some(ref layers) = render_config.render_layers {
            entity_cmd.insert(layers.clone());
        }
    }

    // === Calculate minimap scroll and viewport indicator ===
    let content_height = line_count as f32 * line_height;
    let visible_lines = (viewport_height / line_height).ceil();

    // Calculate scroll progress (0 = top, 1 = bottom)
    let max_scroll = -(content_height - viewport_height).max(0.0);
    let scroll_progress = if max_scroll < 0.0 {
        (state.scroll_offset / max_scroll).clamp(0.0, 1.0)
    } else {
        0.0
    };

    // Calculate minimap scroll offset
    let minimap_scroll_offset = if total_minimap_content_height > viewport_height {
        // Minimap content exceeds viewport - apply scroll
        let max_minimap_scroll = total_minimap_content_height - viewport_height;
        scroll_progress * max_minimap_scroll
    } else {
        0.0
    };

    // Viewport indicator - shows which part of content is visible in editor
    let visible_fraction = (visible_lines / line_count as f32).min(1.0);
    let indicator_height_in_minimap = visible_fraction * total_minimap_content_height;
    let indicator_position_in_minimap =
        scroll_progress * (total_minimap_content_height - indicator_height_in_minimap);

    // Convert to screen space with scroll applied
    let indicator_screen_y =
        indicator_position_in_minimap - minimap_scroll_offset + content_y_offset;
    let indicator_height = indicator_height_in_minimap.max(minimap_settings.min_indicator_height);

    // Convert to world coordinates
    let indicator_y = viewport_height / 2.0 - indicator_screen_y - indicator_height / 2.0;
    let indicator_translation = Vec3::new(
        minimap_center_x,
        indicator_y,
        minimap_settings.viewport_highlight_z_index,
    );

    // === VIEWPORT HIGHLIGHT (only visible on hover, like VSCode) ===
    let show_highlight = minimap_settings.show_viewport_highlight && hover_state.is_hovered;
    if show_highlight {
        if let Ok((_, mut transform, mut sprite, mut visibility)) = highlight_query.single_mut() {
            sprite.custom_size = Some(Vec2::new(minimap_width, indicator_height));
            transform.translation = indicator_translation;
            *visibility = Visibility::Visible;
        } else {
            let mut entity_cmd = commands.spawn((
                Sprite {
                    color: theme.minimap_viewport_highlight,
                    custom_size: Some(Vec2::new(minimap_width, indicator_height)),
                    ..default()
                },
                Transform::from_translation(indicator_translation),
                MinimapViewportHighlight,
                Name::new("MinimapViewportHighlight"),
                Visibility::Visible,
            ));
            if let Some(ref layers) = render_config.render_layers {
                entity_cmd.insert(layers.clone());
            }
        }
    } else {
        for (_, _, _, mut visibility) in highlight_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
    }

    // === SLIDER (more visible, appears on hover) ===
    let show_slider = minimap_settings.show_slider
        && (!minimap_settings.slider_on_hover_only || hover_state.is_hovered);

    if show_slider {
        // Slider is at a higher Z than highlight
        let slider_translation = Vec3::new(
            minimap_center_x,
            indicator_y,
            minimap_settings.slider_z_index,
        );

        if let Ok((_, mut transform, mut sprite, mut visibility)) = slider_query.single_mut() {
            sprite.custom_size = Some(Vec2::new(minimap_width, indicator_height));
            transform.translation = slider_translation;
            *visibility = Visibility::Visible;
        } else {
            let mut entity_cmd = commands.spawn((
                Sprite {
                    color: theme.minimap_slider,
                    custom_size: Some(Vec2::new(minimap_width, indicator_height)),
                    ..default()
                },
                Transform::from_translation(slider_translation),
                MinimapSlider,
                Name::new("MinimapSlider"),
                Visibility::Visible,
            ));
            if let Some(ref layers) = render_config.render_layers {
                entity_cmd.insert(layers.clone());
            }
        }
    } else {
        for (_, _, _, mut visibility) in slider_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
    }

    // === GPU TEXT RENDERING ===
    // Build GPU mesh for minimap text
    let max_column = minimap_settings.max_column;
    let font_size = minimap_font_size;

    // Calculate visible line range for viewport culling
    let buffer_lines = 100; // Extra lines above/below for smooth scrolling

    // Minimap viewport bounds in screen space (top=0, increases downward) with scroll applied
    let minimap_viewport_top = minimap_scroll_offset;
    let minimap_viewport_bottom = minimap_scroll_offset + viewport_height;

    // Calculate which minimap lines are visible
    let first_visible_minimap_line = ((minimap_viewport_top - content_y_offset)
        / minimap_line_height)
        .floor()
        .max(0.0) as usize;
    let last_visible_minimap_line =
        ((minimap_viewport_bottom - content_y_offset) / minimap_line_height).ceil() as usize;

    let start_line = first_visible_minimap_line.saturating_sub(buffer_lines);
    let end_line = (last_visible_minimap_line + buffer_lines).min(line_count);

    // === LAZY HIGHLIGHTING for minimap (simple version - no cache due to param limit) ===
    #[cfg(feature = "tree-sitter")]
    let highlighted_lines = if syntax.is_available() && end_line > start_line {
        // Extract ONLY the visible minimap text range
        let start_char = state.rope.line_to_char(start_line);
        let end_char = state
            .rope
            .line_to_char(end_line.min(state.rope.len_lines()));
        let visible_text: String = state.rope.slice(start_char..end_char).to_string();
        let start_byte = state.rope.char_to_byte(start_char);

        syntax.highlight_range(
            &visible_text,
            0,
            end_line - start_line,
            start_byte, // Byte offset in the full document
            &theme.syntax,
            theme.foreground,
        )
    } else {
        Vec::new()
    };

    #[cfg(not(feature = "tree-sitter"))]
    let highlighted_lines: Vec<Vec<crate::types::LineSegment>> = Vec::new();

    // Build mesh data
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut vertex_count: u32 = 0;

    // Calculate X position for minimap text rendering (accounting for edge padding)
    // Camera viewport handles panel positioning, so no offset_x here
    let minimap_left_world_x = if minimap_settings.show_on_right {
        // For right side: viewport right edge - minimap width - edge padding, then convert to world coords
        let screen_x = viewport_width - minimap_width - minimap_settings.edge_padding;
        screen_x - viewport_width / 2.0
    } else {
        -viewport_width / 2.0 + minimap_settings.edge_padding
    };

    // Render visible lines
    for line_idx in start_line..end_line {
        let line = state.rope.line(line_idx);
        let line_text: String = line
            .chars()
            .take(max_column)
            .filter(|c| *c != '\n' && *c != '\r')
            .collect();

        if line_text.trim().is_empty() {
            continue;
        }

        // Y position (screen space, top=0) with minimap scroll applied
        let screen_y =
            (line_idx as f32 * minimap_line_height) + content_y_offset - minimap_scroll_offset;

        // Convert to world coordinates
        let world_y = viewport_height / 2.0 - screen_y;

        // Get line color from lazy-highlighted lines
        let relative_line = line_idx.saturating_sub(start_line);
        let line_color = if !highlighted_lines.is_empty()
            && relative_line < highlighted_lines.len()
            && !highlighted_lines[relative_line].is_empty()
        {
            let segments = &highlighted_lines[relative_line];
            segments
                .iter()
                .find(|s| !s.text.trim().is_empty())
                .map(|s| s.color)
                .unwrap_or(theme.foreground)
                .with_alpha(0.8)
        } else {
            theme.foreground.with_alpha(0.6)
        };

        let color_arr = line_color.to_linear().to_f32_array();

        // Render each character as a glyph quad
        let mut x = minimap_left_world_x + 2.0; // Small left padding

        for ch in line_text.chars() {
            if ch == '\t' {
                x += font_size * 0.6 * 4.0;
                continue;
            }

            let key = GlyphKey::new(ch, font_size);
            if let Some(info) =
                atlas.get_or_insert(key, || GlyphRasterizer::rasterize(ch, font_size))
            {
                let glyph_world_x = x + info.offset.x;
                let glyph_world_y = world_y - info.offset.y;

                let w = info.size.x;
                let h = info.size.y;

                // Four corners of the glyph quad
                positions.push([glyph_world_x, glyph_world_y - h, 0.0]); // bottom-left
                positions.push([glyph_world_x + w, glyph_world_y - h, 0.0]); // bottom-right
                positions.push([glyph_world_x + w, glyph_world_y, 0.0]); // top-right
                positions.push([glyph_world_x, glyph_world_y, 0.0]); // top-left

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
        // No visible text, hide mesh
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
    if let Some((entity, minimap_mesh, _)) = mesh_query.iter().next() {
        // Check if we need to rebuild (content changed, scroll changed, or viewport resized)
        // Note: offset_x no longer affects content positioning (handled by camera viewport)
        let scroll_changed = (minimap_mesh.built_at_scroll - state.scroll_offset).abs() > 0.01;
        let viewport_changed = minimap_mesh.built_at_width != viewport.width
            || minimap_mesh.built_at_height != viewport.height;
        let needs_rebuild = minimap_mesh.built_at_version != state.content_version
            || scroll_changed
            || viewport_changed;

        if needs_rebuild {
            let new_mesh_handle = meshes.add(mesh);
            commands
                .entity(entity)
                .insert(bevy::mesh::Mesh2d(new_mesh_handle))
                .insert(GpuMinimapMesh {
                    built_at_version: state.content_version,
                    built_at_scroll: state.scroll_offset,
                    built_at_width: viewport.width,
                    built_at_height: viewport.height,
                    built_at_offset_x: viewport.offset_x,
                })
                .insert(Visibility::Visible);
        } else {
            commands.entity(entity).insert(Visibility::Visible);
        }
    } else {
        // Create new minimap mesh entity
        let mesh_handle = meshes.add(mesh);
        let mut entity_cmd = commands.spawn((
            bevy::mesh::Mesh2d(mesh_handle),
            MeshMaterial2d(material_handle.clone()),
            Transform::from_translation(Vec3::new(0.0, 0.0, 5.2)),
            GpuMinimapMesh {
                built_at_version: state.content_version,
                built_at_scroll: state.scroll_offset,
                built_at_width: viewport.width,
                built_at_height: viewport.height,
                built_at_offset_x: viewport.offset_x,
            },
            Name::new("GpuMinimapMesh"),
            Visibility::Visible,
        ));
        if let Some(ref layers) = render_config.render_layers {
            entity_cmd.insert(layers.clone());
        }
    }
}
