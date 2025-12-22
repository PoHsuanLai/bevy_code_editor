//! Minimap rendering and interaction

use bevy::prelude::*;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::asset::RenderAssetUsages;
use bevy::sprite_render::MeshMaterial2d;
use crate::settings::EditorSettings;
use crate::types::*;
use crate::gpu_text::{GlyphAtlas, GlyphKey, GlyphRasterizer, TextMaterial, TextRenderState};
use super::scrollbar::Scrollbar;

pub(crate) fn update_minimap_hover(
    windows: Query<&Window>,
    viewport: Res<ViewportDimensions>,
    settings: Res<EditorSettings>,
    mut hover_state: ResMut<MinimapHoverState>,
) {
    if !settings.minimap.enabled {
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
    let minimap_width = settings.minimap.width;

    // Check if cursor is over the minimap area
    let is_over_minimap = if settings.minimap.show_on_right {
        cursor_pos.x >= viewport_width - minimap_width
    } else {
        cursor_pos.x <= minimap_width
    };

    hover_state.is_hovered = is_over_minimap;
}

/// Handle mouse clicks and drags on the minimap for click-to-scroll and drag-to-scroll
pub(crate) fn handle_minimap_mouse(
    windows: Query<&Window>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut state: ResMut<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    hover_state: Res<MinimapHoverState>,
    mut drag_state: ResMut<MinimapDragState>,
) {
    if !settings.minimap.enabled {
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
    let line_height = settings.font.line_height;

    // Minimap settings (same as in update_minimap)
    let minimap_line_height = settings.minimap.line_height;
    let total_minimap_content_height = line_count as f32 * minimap_line_height;

    // Content Y offset for centering
    let content_y_offset = if settings.minimap.center_when_short && total_minimap_content_height < viewport_height {
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
    }

    // Handle mouse button press on minimap
    if mouse_button.just_pressed(MouseButton::Left) && hover_state.is_hovered {
        drag_state.is_dragging = true;
    }

    // Handle click or drag on minimap
    if (mouse_button.just_pressed(MouseButton::Left) && hover_state.is_hovered) ||
       (drag_state.is_dragging && mouse_button.pressed(MouseButton::Left)) {
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
    mut state: ResMut<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    hover_state: Res<MinimapHoverState>,
    mut atlas: ResMut<GlyphAtlas>,
    render_state: Res<TextRenderState>,
    mut materials: ResMut<Assets<TextMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mesh_query: Query<(Entity, &GpuMinimapMesh, &bevy::mesh::Mesh2d)>,
    mut syntax: ResMut<super::SyntaxResource>,
    mut bg_query: Query<(Entity, &mut Transform, &mut Sprite, &mut Visibility), (With<MinimapBackground>, Without<MinimapSlider>, Without<MinimapViewportHighlight>)>,
    mut slider_query: Query<(Entity, &mut Transform, &mut Sprite, &mut Visibility), (With<MinimapSlider>, Without<MinimapBackground>, Without<MinimapViewportHighlight>)>,
    mut highlight_query: Query<(Entity, &mut Transform, &mut Sprite, &mut Visibility), (With<MinimapViewportHighlight>, Without<MinimapBackground>, Without<MinimapSlider>)>,
    mut scrollbar_query: Query<&mut Scrollbar, With<MinimapScrollbar>>,
) {
    // Hide everything if minimap is disabled
    if !settings.minimap.enabled {
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
        // Disable scrollbar
        if let Ok(mut scrollbar) = scrollbar_query.single_mut() {
            scrollbar.enabled = false;
        }
        return;
    }

    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;
    let minimap_width = settings.minimap.width;
    let line_count = state.rope.len_lines();
    let line_height = settings.font.line_height;

    // Minimap text settings - tiny font like VSCode
    let minimap_line_height = settings.minimap.line_height;
    let minimap_font_size = settings.minimap.font_size;

    // Calculate total content height in minimap (unscaled)
    let total_minimap_content_height = line_count as f32 * minimap_line_height;

    // Minimap uses fixed size - content scrolls when it exceeds viewport
    let effective_minimap_height = total_minimap_content_height.min(viewport_height);

    // Vertical offset for centering when content is short
    let content_y_offset = if settings.minimap.center_when_short && total_minimap_content_height < viewport_height {
        (viewport_height - total_minimap_content_height) / 2.0
    } else {
        0.0
    };

    // Minimap X position (right or left side)
    // When on right, shift left to make room for scrollbar with spacing
    let minimap_left_x = if settings.minimap.show_on_right {
        viewport_width / 2.0 - minimap_width - settings.minimap.scrollbar_width - settings.minimap.scrollbar_spacing + settings.minimap.padding
    } else {
        -viewport_width / 2.0 + settings.minimap.padding
    };

    let minimap_center_x = if settings.minimap.show_on_right {
        viewport_width / 2.0 - minimap_width / 2.0 - settings.minimap.scrollbar_width - settings.minimap.scrollbar_spacing
    } else {
        -viewport_width / 2.0 + minimap_width / 2.0
    };

    // === BACKGROUND ===
    if let Ok((_, mut transform, mut sprite, mut visibility)) = bg_query.single_mut() {
        sprite.custom_size = Some(Vec2::new(minimap_width, viewport_height));
        transform.translation = Vec3::new(minimap_center_x, 0.0, settings.minimap.background_z_index);
        *visibility = Visibility::Visible;
    } else {
        commands.spawn((
            Sprite {
                color: settings.theme.minimap_background,
                custom_size: Some(Vec2::new(minimap_width, viewport_height)),
                ..default()
            },
            Transform::from_translation(Vec3::new(minimap_center_x, 0.0, settings.minimap.background_z_index)),
            MinimapBackground,
            Name::new("MinimapBackground"),
            Visibility::Visible,
        ));
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
    let indicator_position_in_minimap = scroll_progress * (total_minimap_content_height - indicator_height_in_minimap);

    // Convert to screen space with scroll applied
    let indicator_screen_y = indicator_position_in_minimap - minimap_scroll_offset + content_y_offset;
    let indicator_height = indicator_height_in_minimap.max(settings.minimap.min_indicator_height);

    // Convert to world coordinates
    let indicator_y = viewport_height / 2.0 - indicator_screen_y - indicator_height / 2.0;
    let indicator_translation = Vec3::new(minimap_center_x, indicator_y, settings.minimap.viewport_highlight_z_index);

    // === VIEWPORT HIGHLIGHT (only visible on hover, like VSCode) ===
    let show_highlight = settings.minimap.show_viewport_highlight && hover_state.is_hovered;
    if show_highlight {
        if let Ok((_, mut transform, mut sprite, mut visibility)) = highlight_query.single_mut() {
            sprite.custom_size = Some(Vec2::new(minimap_width, indicator_height));
            transform.translation = indicator_translation;
            *visibility = Visibility::Visible;
        } else {
            commands.spawn((
                Sprite {
                    color: settings.theme.minimap_viewport_highlight,
                    custom_size: Some(Vec2::new(minimap_width, indicator_height)),
                    ..default()
                },
                Transform::from_translation(indicator_translation),
                MinimapViewportHighlight,
                Name::new("MinimapViewportHighlight"),
                Visibility::Visible,
            ));
        }
    } else {
        for (_, _, _, mut visibility) in highlight_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
    }

    // === SLIDER (more visible, appears on hover) ===
    let show_slider = settings.minimap.show_slider &&
        (!settings.minimap.slider_on_hover_only || hover_state.is_hovered);

    if show_slider {
        // Slider is at a higher Z than highlight
        let slider_translation = Vec3::new(minimap_center_x, indicator_y, settings.minimap.slider_z_index);

        if let Ok((_, mut transform, mut sprite, mut visibility)) = slider_query.single_mut() {
            sprite.custom_size = Some(Vec2::new(minimap_width, indicator_height));
            transform.translation = slider_translation;
            *visibility = Visibility::Visible;
        } else {
            commands.spawn((
                Sprite {
                    color: settings.theme.minimap_slider,
                    custom_size: Some(Vec2::new(minimap_width, indicator_height)),
                    ..default()
                },
                Transform::from_translation(slider_translation),
                MinimapSlider,
                Name::new("MinimapSlider"),
                Visibility::Visible,
            ));
        }
    } else {
        for (_, _, _, mut visibility) in slider_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
    }

    // === SCROLLBAR (VSCode-style, separate from minimap) ===
    // Update or create scrollbar component
    if let Ok(mut scrollbar) = scrollbar_query.single_mut() {
        // Update existing scrollbar
        scrollbar.enabled = total_minimap_content_height > viewport_height;
        scrollbar.x = if settings.minimap.show_on_right {
            viewport_width / 2.0 - settings.minimap.scrollbar_width / 2.0
        } else {
            -viewport_width / 2.0 + minimap_width + settings.minimap.scrollbar_width / 2.0
        };
        scrollbar.y = 0.0;
        scrollbar.width = settings.minimap.scrollbar_width;
        scrollbar.track_height = viewport_height;
        scrollbar.scroll_progress = scroll_progress;
        scrollbar.visible_fraction = viewport_height / total_minimap_content_height;
        scrollbar.min_thumb_height = settings.minimap.scrollbar_min_thumb_height;
        scrollbar.z_index = settings.minimap.scrollbar_z_index;
        scrollbar.track_color = settings.minimap.scrollbar_track_color;
        scrollbar.thumb_color = settings.minimap.scrollbar_thumb_color;
        scrollbar.border_radius = settings.minimap.scrollbar_border_radius;
    } else {
        // Create new scrollbar entity
        let scrollbar_x = if settings.minimap.show_on_right {
            viewport_width / 2.0 - settings.minimap.scrollbar_width / 2.0
        } else {
            -viewport_width / 2.0 + minimap_width + settings.minimap.scrollbar_width / 2.0
        };

        commands.spawn((
            Scrollbar {
                x: scrollbar_x,
                y: 0.0,
                width: settings.minimap.scrollbar_width,
                track_height: viewport_height,
                scroll_progress,
                visible_fraction: viewport_height / total_minimap_content_height,
                min_thumb_height: settings.minimap.scrollbar_min_thumb_height,
                z_index: settings.minimap.scrollbar_z_index,
                track_color: settings.minimap.scrollbar_track_color,
                thumb_color: settings.minimap.scrollbar_thumb_color,
                enabled: total_minimap_content_height > viewport_height,
                border_radius: settings.minimap.scrollbar_border_radius,
            },
            MinimapScrollbar,
            Name::new("MinimapScrollbar"),
        ));
    }

    // === GPU TEXT RENDERING ===
    // Build GPU mesh for minimap text
    let max_column = settings.minimap.max_column;
    let font_size = minimap_font_size;

    // Calculate visible line range for viewport culling
    let buffer_lines = 100; // Extra lines above/below for smooth scrolling

    // Minimap viewport bounds in screen space (top=0, increases downward) with scroll applied
    let minimap_viewport_top = minimap_scroll_offset;
    let minimap_viewport_bottom = minimap_scroll_offset + viewport_height;

    // Calculate which minimap lines are visible
    let first_visible_minimap_line = ((minimap_viewport_top - content_y_offset) / minimap_line_height)
        .floor()
        .max(0.0) as usize;
    let last_visible_minimap_line = ((minimap_viewport_bottom - content_y_offset) / minimap_line_height)
        .ceil() as usize;

    let start_line = first_visible_minimap_line.saturating_sub(buffer_lines);
    let end_line = (last_visible_minimap_line + buffer_lines).min(line_count);

    // === LAZY HIGHLIGHTING for minimap (simple version - no cache due to param limit) ===
    #[cfg(feature = "tree-sitter")]
    let highlighted_lines = if syntax.is_available() && end_line > start_line {
        // Extract ONLY the visible minimap text range
        let start_char = state.rope.line_to_char(start_line);
        let end_char = state.rope.line_to_char(end_line.min(state.rope.len_lines()));
        let visible_text: String = state.rope.slice(start_char..end_char).to_string();
        let start_byte = state.rope.char_to_byte(start_char);

        syntax.highlight_range(
            &visible_text,
            0,
            end_line - start_line,
            start_byte, // Byte offset in the full document
            &settings.theme.syntax,
            settings.theme.foreground,
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

    // FIX: Calculate correct X position for right side (accounting for scrollbar with spacing)
    let minimap_left_world_x = if settings.minimap.show_on_right {
        // For right side: viewport right edge - minimap width - scrollbar width - spacing, then convert to world coords
        let screen_x = viewport_width - minimap_width - settings.minimap.scrollbar_width - settings.minimap.scrollbar_spacing;
        screen_x - viewport_width / 2.0 + viewport.offset_x
    } else {
        -viewport_width / 2.0 + viewport.offset_x
    };

    // Render visible lines
    for line_idx in start_line..end_line {
        let line = state.rope.line(line_idx);
        let line_text: String = line.chars()
            .take(max_column)
            .filter(|c| *c != '\n' && *c != '\r')
            .collect();

        if line_text.trim().is_empty() {
            continue;
        }

        // Y position (screen space, top=0) with minimap scroll applied
        let screen_y = (line_idx as f32 * minimap_line_height) + content_y_offset - minimap_scroll_offset;

        // Convert to world coordinates
        let world_y = viewport_height / 2.0 - screen_y;

        // Get line color from lazy-highlighted lines
        let relative_line = line_idx.saturating_sub(start_line);
        let line_color = if !highlighted_lines.is_empty() && relative_line < highlighted_lines.len() && !highlighted_lines[relative_line].is_empty() {
            let segments = &highlighted_lines[relative_line];
            segments.iter()
                .find(|s| !s.text.trim().is_empty())
                .map(|s| s.color)
                .unwrap_or(settings.theme.foreground)
                .with_alpha(0.8)
        } else {
            settings.theme.foreground.with_alpha(0.6)
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
            if let Some(info) = atlas.get_or_insert(key, || {
                GlyphRasterizer::rasterize(ch, font_size)
            }) {
                let glyph_world_x = x + info.offset.x;
                let glyph_world_y = world_y - info.offset.y;

                let w = info.size.x;
                let h = info.size.y;

                // Four corners of the glyph quad
                positions.push([glyph_world_x, glyph_world_y - h, 0.0]);       // bottom-left
                positions.push([glyph_world_x + w, glyph_world_y - h, 0.0]);   // bottom-right
                positions.push([glyph_world_x + w, glyph_world_y, 0.0]);       // top-right
                positions.push([glyph_world_x, glyph_world_y, 0.0]);           // top-left

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
        // Check if we need to rebuild (content changed or scroll changed)
        let scroll_changed = (minimap_mesh.built_at_scroll - state.scroll_offset).abs() > 0.01;
        let needs_rebuild = minimap_mesh.built_at_version != state.content_version || scroll_changed;

        if needs_rebuild {
            let new_mesh_handle = meshes.add(mesh);
            commands.entity(entity)
                .insert(bevy::mesh::Mesh2d(new_mesh_handle))
                .insert(GpuMinimapMesh {
                    built_at_version: state.content_version,
                    built_at_scroll: state.scroll_offset,
                })
                .insert(Visibility::Visible);
        } else {
            commands.entity(entity).insert(Visibility::Visible);
        }
    } else {
        // Create new minimap mesh entity
        let mesh_handle = meshes.add(mesh);
        commands.spawn((
            bevy::mesh::Mesh2d(mesh_handle),
            MeshMaterial2d(material_handle.clone()),
            Transform::from_translation(Vec3::new(0.0, 0.0, 5.2)),
            GpuMinimapMesh {
                built_at_version: state.content_version,
                built_at_scroll: state.scroll_offset,
            },
            Name::new("GpuMinimapMesh"),
            Visibility::Visible,
        ));
    }
}

/// Update minimap to show search match highlights
pub(crate) fn update_minimap_find_highlights(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    find_state: Res<FindState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    mut highlight_query: Query<(Entity, &mut Transform, &mut Sprite, &mut Visibility, &MinimapFindHighlight)>,
) {
    // Hide all if minimap disabled or no active search
    if !settings.minimap.enabled || !find_state.active || find_state.matches.is_empty() {
        for (_, _, _, mut visibility, _) in highlight_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    let viewport_height = viewport.height as f32;
    let viewport_width = viewport.width as f32;
    let minimap_width = settings.minimap.width;
    let line_count = state.rope.len_lines();

    // Minimap scaling (same as in update_minimap)
    let minimap_line_height = 4.0;
    let total_minimap_content_height = line_count as f32 * minimap_line_height;
    let scale = if total_minimap_content_height > viewport_height {
        viewport_height / total_minimap_content_height
    } else {
        1.0
    };
    let scaled_line_height = minimap_line_height * scale;

    // Content Y offset for centering
    let content_y_offset = if settings.minimap.center_when_short && total_minimap_content_height < viewport_height {
        (viewport_height - total_minimap_content_height) / 2.0
    } else {
        0.0
    };

    // Minimap X position
    let minimap_center_x = if settings.minimap.show_on_right {
        viewport_width / 2.0 - minimap_width / 2.0
    } else {
        -viewport_width / 2.0 + minimap_width / 2.0
    };

    // Collect lines with matches (deduplicated)
    let mut match_lines: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for m in &find_state.matches {
        let line = state.rope.char_to_line(m.start);
        match_lines.insert(line);
    }

    // Collect existing highlight entities by line index
    let mut existing_by_line: std::collections::HashMap<usize, Entity> = std::collections::HashMap::new();
    for (entity, _, _, _, highlight) in highlight_query.iter() {
        existing_by_line.insert(highlight.line_index, entity);
    }

    let mut used_lines: std::collections::HashSet<usize> = std::collections::HashSet::new();

    // Create/update highlight entities for each line with matches
    for line_idx in &match_lines {
        used_lines.insert(*line_idx);

        // Y position from top, with centering offset
        let line_y = viewport_height / 2.0 - (*line_idx as f32 * scaled_line_height) - scaled_line_height / 2.0 - content_y_offset;
        let translation = Vec3::new(minimap_center_x, line_y, 5.1); // Behind text (5.2)

        if let Some(entity) = existing_by_line.get(line_idx) {
            // Update existing
            if let Ok((_, mut transform, mut sprite, mut visibility, _)) = highlight_query.get_mut(*entity) {
                transform.translation = translation;
                sprite.custom_size = Some(Vec2::new(minimap_width, scaled_line_height.max(2.0)));
                sprite.color = settings.theme.find_match.with_alpha(0.5);
                *visibility = Visibility::Visible;
            }
        } else {
            // Spawn new highlight
            commands.spawn((
                Sprite {
                    color: settings.theme.find_match.with_alpha(0.5),
                    custom_size: Some(Vec2::new(minimap_width, scaled_line_height.max(2.0))),
                    ..default()
                },
                Transform::from_translation(translation),
                MinimapFindHighlight { line_index: *line_idx },
                Name::new(format!("MinimapFindHighlight_{}", line_idx)),
                Visibility::Visible,
            ));
        }
    }

    // Hide unused highlight entities
    for (_, _, _, mut visibility, highlight) in highlight_query.iter_mut() {
        if !used_lines.contains(&highlight.line_index) {
            *visibility = Visibility::Hidden;
        }
    }
}
