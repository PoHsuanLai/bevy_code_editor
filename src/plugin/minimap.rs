//! Code Minimap
//!
//! Renders a small overview of the code on the right side.
//! Uses GPU instanced rendering for performance.

use super::editor_ui_plugin::EditorRenderConfig;
use super::gpu_text_instanced::{GlyphBatchComponent, GlyphInstance};
use crate::gpu_text::{GlyphAtlas, GlyphKey, GlyphRasterizer};
use crate::settings::*;
use crate::types::*;
use bevy::prelude::*;

pub struct MinimapPlugin;

impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(feature = "minimap")]
        {
            // Minimap input goes in InputSet
            app.add_systems(
                Update,
                (update_minimap_hover, handle_minimap_mouse)
                    .chain()
                    .in_set(super::InputSet),
            );

            // Minimap rendering goes in RenderingSet
            app.add_systems(
                Update,
                update_minimap
                    .run_if(crate::gpu_text::atlas_ready)
                    .in_set(super::RenderingSet),
            );
        }
    }
}

/// Marker component for the GPU minimap batch entity
#[derive(Component)]
pub struct GpuMinimapBatch {
    /// Content version when this batch was built
    pub built_at_version: u64,
    /// Scroll offset when this batch was built
    pub built_at_scroll: f32,
    /// Viewport dimensions when built
    pub built_at_width: u32,
    pub built_at_height: u32,
}

/// Component for the minimap viewport highlight
#[derive(Component)]
pub struct MinimapViewportHighlight;

/// Component for the minimap background
#[derive(Component)]
pub struct MinimapBackground;

/// Update minimap hover state based on mouse position
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

/// Handle mouse clicks on minimap
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

    // Minimap settings
    let minimap_line_height = minimap_settings.line_height;
    let total_minimap_content_height = line_count as f32 * minimap_line_height;

    // Content Y offset for centering
    let content_y_offset =
        if minimap_settings.center_when_short && total_minimap_content_height < viewport_height {
            (viewport_height - total_minimap_content_height) / 2.0
        } else {
            0.0
        };

    // Calculate minimap scroll offset
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

        if is_over_highlight {
            drag_state.is_dragging_highlight = true;
            drag_state.drag_start_y = cursor_pos.y;
            drag_state.drag_start_scroll = state.scroll_offset;
        } else {
            drag_state.is_dragging_highlight = false;
            // Click to scroll - calculate which line was clicked
            let click_y_from_top = viewport_height - cursor_pos.y;
            let adjusted_y = click_y_from_top + minimap_scroll_offset - content_y_offset;
            let line_index = (adjusted_y / minimap_line_height).floor() as usize;

            if line_index < line_count {
                // Center the viewport on this line
                let target_line_y = line_index as f32 * line_height;
                state.target_scroll_offset = -(target_line_y - viewport_height / 2.0);
                state.needs_scroll_update = true;
            }
        }
    }

    // Handle dragging the viewport highlight
    if drag_state.is_dragging
        && drag_state.is_dragging_highlight
        && mouse_button.pressed(MouseButton::Left)
    {
        let delta_y = cursor_pos.y - drag_state.drag_start_y;
        let minimap_to_content_ratio = if total_minimap_content_height > 0.0 {
            content_height / total_minimap_content_height
        } else {
            1.0
        };

        let scroll_delta = -delta_y * minimap_to_content_ratio;
        state.target_scroll_offset =
            (drag_state.drag_start_scroll + scroll_delta).clamp(max_scroll, 0.0);
        state.needs_scroll_update = true;
    }
}

/// GPU-accelerated minimap rendering system
pub(crate) fn update_minimap(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    font: Res<FontSettings>,
    theme: Res<ThemeSettings>,
    minimap_settings: Res<MinimapSettings>,
    viewport: Res<ViewportDimensions>,
    render_config: Res<EditorRenderConfig>,
    mut atlas: ResMut<GlyphAtlas>,
    mut images: ResMut<Assets<Image>>,
    mut batch_query: Query<
        (Entity, &mut GpuMinimapBatch, &mut GlyphBatchComponent, &mut Transform),
        (With<GpuMinimapBatch>, Without<MinimapViewportHighlight>),
    >,
    mut bg_query: Query<
        (Entity, &mut Visibility),
        (With<MinimapBackground>, Without<MinimapViewportHighlight>),
    >,
    mut highlight_query: Query<
        (Entity, &mut Transform, &mut Sprite, &mut Visibility),
        With<MinimapViewportHighlight>,
    >,
) {
    let viewport_width = viewport.width as f32;

    // Hide if minimap is disabled or viewport too narrow
    if !minimap_settings.should_show(viewport_width) {
        for (entity, _) in bg_query.iter() {
            commands.entity(entity).insert(Visibility::Hidden);
        }
        for (entity, _, _, _) in batch_query.iter() {
            commands.entity(entity).insert(Visibility::Hidden);
        }
        for (_, _, _, mut visibility) in highlight_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    let viewport_height = viewport.height as f32;
    let minimap_width = minimap_settings.width;
    let line_count = state.rope.len_lines();
    let minimap_line_height = minimap_settings.line_height;
    let minimap_font_size = minimap_settings.font_size;
    let char_width = minimap_font_size * 0.6; // Approximate char width

    // Check if we need to rebuild
    let needs_rebuild = if let Some((_, batch, _, _)) = batch_query.iter().next() {
        batch.built_at_version != state.content_version
            || (batch.built_at_scroll - state.scroll_offset).abs() > 0.1
            || batch.built_at_width != viewport.width
            || batch.built_at_height != viewport.height
    } else {
        true
    };

    if !needs_rebuild && !state.needs_update {
        return;
    }

    // Calculate minimap position
    let minimap_center_x = if minimap_settings.show_on_right {
        viewport.world_left() + viewport_width - minimap_width / 2.0 - minimap_settings.edge_padding
    } else {
        viewport.world_left() + minimap_width / 2.0 + minimap_settings.edge_padding
    };

    // Calculate which lines are visible
    let total_minimap_height = line_count as f32 * minimap_line_height;
    let content_y_offset =
        if minimap_settings.center_when_short && total_minimap_height < viewport_height {
            (viewport_height - total_minimap_height) / 2.0
        } else {
            0.0
        };

    // Calculate which lines are visible in the minimap viewport
    let minimap_top = viewport.world_top();
    let minimap_bottom = viewport.world_top() - viewport_height;

    // Render all visible lines (minimap shows overview, so render more)
    let mut instances = Vec::new();

    for line_idx in 0..line_count.min(500) {
        // Limit to 500 lines for performance
        let line = state.rope.line(line_idx);
        let line_str = line.to_string();

        // Calculate Y position
        let y_offset = content_y_offset + line_idx as f32 * minimap_line_height;
        let world_y = viewport.world_top() - y_offset - minimap_line_height;

        // Skip lines that are completely outside the viewport
        if world_y + minimap_line_height < minimap_bottom || world_y > minimap_top {
            continue;
        }

        // Render characters (simplified, no syntax highlighting in minimap)
        let mut x_offset = 0.0;
        let color = theme.foreground.to_linear();
        let color_arr = [color.red, color.green, color.blue, 0.3]; // Lower alpha for minimap

        // Calculate minimap bounds for clipping
        let minimap_left = minimap_center_x - minimap_width / 2.0;
        let minimap_right = minimap_center_x + minimap_width / 2.0;
        let minimap_top = viewport.world_top() - content_y_offset;
        let minimap_bottom = viewport.world_top() - viewport_height;

        for ch in line_str.chars() {
            if ch == '\n' || ch == '\r' {
                break;
            }

            // Stop if we've exceeded minimap width
            if x_offset >= minimap_width {
                break;
            }

            if ch == '\t' {
                x_offset += char_width * 4.0;
                continue;
            }
            if ch.is_whitespace() {
                x_offset += char_width;
                continue;
            }

            let key = GlyphKey::new(ch, minimap_font_size);
            if let Some(info) =
                atlas.get_or_insert(key, || GlyphRasterizer::rasterize(ch, minimap_font_size))
            {
                let world_x = minimap_left + x_offset + info.offset.x;

                // Clip characters that would go outside minimap bounds
                if world_x >= minimap_left
                    && world_x + info.size.x <= minimap_right
                    && world_y >= minimap_bottom
                    && world_y + info.size.y <= minimap_top
                {
                    instances.push(GlyphInstance {
                        position: Vec2::new(world_x, world_y),
                        uv_min: info.uv_min,
                        uv_max: info.uv_max,
                        size: info.size,
                        color: color_arr,
                        z_index: minimap_settings.text_z_index as f32,
                        _padding: [0.0; 3],
                    });
                }

                x_offset += char_width;
            }
        }
    }

    // Update atlas texture
    atlas.update_texture(&mut images);

    // Create or update batch
    if let Some((entity, mut batch, mut batch_component, mut transform)) = batch_query.iter_mut().next() {
        batch.built_at_version = state.content_version;
        batch.built_at_scroll = state.scroll_offset;
        batch.built_at_width = viewport.width;
        batch.built_at_height = viewport.height;
        batch_component.instances = std::sync::Arc::new(instances);
        batch_component.atlas_texture = atlas.texture.clone();
        transform.translation.z = minimap_settings.text_z_index as f32;
        commands.entity(entity).insert(Visibility::Visible);
    } else {
        let mut entity_cmd = commands.spawn((
            GpuMinimapBatch {
                built_at_version: state.content_version,
                built_at_scroll: state.scroll_offset,
                built_at_width: viewport.width,
                built_at_height: viewport.height,
            },
            GlyphBatchComponent {
                instances: std::sync::Arc::new(instances),
                atlas_texture: atlas.texture.clone(),
            },
            Transform::from_translation(Vec3::new(0.0, 0.0, minimap_settings.text_z_index as f32)),
            GlobalZIndex(minimap_settings.text_z_index),
            Visibility::Visible,
            Name::new("MinimapTextBatch"),
        ));

        if let Some(ref layers) = render_config.render_layers {
            entity_cmd.insert(layers.clone());
        }
    }

    // Update or spawn background
    if let Ok((entity, mut visibility)) = bg_query.single_mut() {
        *visibility = Visibility::Visible;
        // Ensure GlobalZIndex is set (may not exist on first run)
        commands.entity(entity).insert(GlobalZIndex(minimap_settings.background_z_index));
    } else {
        let mut entity_cmd = commands.spawn((
            Sprite {
                color: theme.minimap_background,
                custom_size: Some(Vec2::new(minimap_width, viewport_height)),
                ..default()
            },
            Transform::from_translation(Vec3::new(minimap_center_x, 0.0, minimap_settings.background_z_index as f32)),
            GlobalZIndex(minimap_settings.background_z_index),
            MinimapBackground,
            Visibility::Visible,
            Name::new("MinimapBackground"),
        ));

        if let Some(ref layers) = render_config.render_layers {
            entity_cmd.insert(layers.clone());
        }
    }

    // Update viewport highlight (shows which part of code is visible)
    if minimap_settings.show_viewport_highlight {
        let content_height = line_count as f32 * font.line_height;
        let visible_lines = (viewport_height / font.line_height).floor();
        let scroll_progress = if content_height > viewport_height {
            (-state.scroll_offset / (content_height - viewport_height)).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let highlight_height =
            (visible_lines / line_count as f32 * total_minimap_height).min(viewport_height);

        // Calculate Y position in minimap space:
        // - Minimap background is centered at y=0, extends from -viewport_height/2 to +viewport_height/2
        // - Content starts at content_y_offset from the top
        // - Highlight position = top of minimap - content_y_offset - scroll_offset - half highlight height (for centering)
        let scroll_offset_in_minimap = scroll_progress * (total_minimap_height - highlight_height);
        let minimap_top = viewport_height / 2.0;
        let highlight_y = minimap_top - content_y_offset - scroll_offset_in_minimap - highlight_height / 2.0;

        if let Ok((entity, mut transform, mut sprite, mut visibility)) = highlight_query.single_mut() {
            sprite.custom_size = Some(Vec2::new(minimap_width, highlight_height));
            transform.translation = Vec3::new(minimap_center_x, highlight_y, minimap_settings.viewport_highlight_z_index as f32);
            *visibility = Visibility::Visible;
            // Ensure GlobalZIndex is set (may not exist on first run)
            commands.entity(entity).insert(GlobalZIndex(minimap_settings.viewport_highlight_z_index));
        } else {
            let mut entity_cmd = commands.spawn((
                Sprite {
                    color: theme.minimap_slider,
                    custom_size: Some(Vec2::new(minimap_width, highlight_height)),
                    ..default()
                },
                Transform::from_translation(Vec3::new(minimap_center_x, highlight_y, minimap_settings.viewport_highlight_z_index as f32)),
                GlobalZIndex(minimap_settings.viewport_highlight_z_index),
                MinimapViewportHighlight,
                Visibility::Visible,
                Name::new("MinimapViewportHighlight"),
            ));

            if let Some(ref layers) = render_config.render_layers {
                entity_cmd.insert(layers.clone());
            }
        }
    }
}
