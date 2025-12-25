//! Standalone scrollbar plugin
//!
//! Provides a reusable scrollbar component that can be added to any entity

use bevy::prelude::*;

/// Scrollbar plugin - manages scrollbar rendering and interaction
pub struct ScrollbarPlugin;

impl Plugin for ScrollbarPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ScrollbarDragState::default());
        // Scrollbar mouse input goes in InputSet
        app.add_systems(Update, handle_scrollbar_mouse.in_set(crate::plugin::InputSet));
        // Scrollbar visual updates go in RenderingSet
        app.add_systems(Update, update_scrollbars.in_set(crate::plugin::RenderingSet));
    }
}

/// Resource to track scrollbar drag state
#[derive(Resource, Default)]
pub struct ScrollbarDragState {
    /// Whether we're currently dragging a scrollbar
    pub is_dragging: bool,
    /// The entity of the scrollbar being dragged
    pub dragging_entity: Option<Entity>,
    /// Initial mouse Y position when drag started
    pub drag_start_y: f32,
    /// Initial scroll offset when drag started
    pub drag_start_scroll: f32,
}

/// Check if mouse is over any scrollbar (used as a run condition)
pub fn mouse_not_over_scrollbar(
    windows: Query<&Window>,
    scrollbar_query: Query<&Scrollbar, With<EditorScrollbar>>,
) -> bool {
    let Ok(window) = windows.single() else { return true; };
    let Some(cursor_pos_window) = window.cursor_position() else { return true; };

    // Convert to world coordinates
    let cursor_x = cursor_pos_window.x - window.width() / 2.0;

    // Check if over any scrollbar (entire track area, not just thumb)
    for scrollbar in scrollbar_query.iter() {
        if !scrollbar.enabled {
            continue;
        }

        let scrollbar_left = scrollbar.x - scrollbar.width / 2.0;
        let scrollbar_right = scrollbar.x + scrollbar.width / 2.0;

        if cursor_x >= scrollbar_left && cursor_x <= scrollbar_right {
            return false; // Mouse IS over scrollbar
        }
    }

    true // Mouse is NOT over scrollbar
}

/// Component that holds scrollbar configuration
#[derive(Component)]
pub struct Scrollbar {
    /// X position in world coordinates
    pub x: f32,
    /// Y position (center)
    pub y: f32,
    /// Width of the scrollbar
    pub width: f32,
    /// Total height of the scrollbar track
    pub track_height: f32,
    /// Visible fraction (0.0-1.0) - determines thumb size
    pub visible_fraction: f32,
    /// Minimum thumb height in pixels
    pub min_thumb_height: f32,
    /// Z-index for rendering
    pub z_index: f32,
    /// Track color
    pub track_color: Color,
    /// Thumb color
    pub thumb_color: Color,
    /// Whether the scrollbar is enabled
    pub enabled: bool,
    /// Border radius for rounded corners
    pub border_radius: f32,
}

impl Default for Scrollbar {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            track_height: 800.0,
            visible_fraction: 1.0,
            min_thumb_height: 30.0,
            z_index: 10.0,
            track_color: Color::srgba(0.2, 0.2, 0.2, 0.3),
            thumb_color: Color::srgba(0.5, 0.5, 0.5, 0.6),
            enabled: true,
            border_radius: 5.0,
        }
    }
}

/// Marker for scrollbar track entity
#[derive(Component)]
pub(crate) struct ScrollbarTrack {
    pub(crate) parent: Entity,
}

/// Marker for scrollbar thumb entity
#[derive(Component)]
pub struct ScrollbarThumb {
    pub parent: Entity,
}

/// System to handle mouse interaction with scrollbars (clicking and dragging)
fn handle_scrollbar_mouse(
    windows: Query<&Window>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut drag_state: ResMut<ScrollbarDragState>,
    mut state: ResMut<crate::types::CodeEditorState>,
    scrollbar_query: Query<(Entity, &Scrollbar)>,
    _track_query: Query<(&ScrollbarTrack, &Transform, &Sprite)>,
    thumb_query: Query<(&ScrollbarThumb, &Transform, &Sprite)>,
    font: Res<crate::settings::FontSettings>,
    viewport: Res<crate::types::ViewportDimensions>,
) {
    let Ok(window) = windows.single() else { return; };
    let Some(cursor_pos_window) = window.cursor_position() else {
        // No cursor, release drag if active
        if mouse_button.just_released(MouseButton::Left) {
            drag_state.is_dragging = false;
            drag_state.dragging_entity = None;
        }
        return;
    };

    // Convert window coordinates to world coordinates
    let cursor_y = cursor_pos_window.y - window.height() / 2.0;
    let cursor_x = cursor_pos_window.x - window.width() / 2.0;

    // Handle mouse button just pressed - check if clicking on a thumb
    if mouse_button.just_pressed(MouseButton::Left) {
        for (thumb, transform, sprite) in thumb_query.iter() {
            let Some(size) = sprite.custom_size else { continue; };
            let thumb_x = transform.translation.x;
            let thumb_y = transform.translation.y;
            let thumb_half_width = size.x / 2.0;
            let thumb_half_height = size.y / 2.0;

            // Check if cursor is over this thumb
            if cursor_x >= thumb_x - thumb_half_width && cursor_x <= thumb_x + thumb_half_width &&
               cursor_y >= thumb_y - thumb_half_height && cursor_y <= thumb_y + thumb_half_height {
                // Start dragging
                if let Ok((entity, _scrollbar)) = scrollbar_query.get(thumb.parent) {
                    drag_state.is_dragging = true;
                    drag_state.dragging_entity = Some(entity);
                    drag_state.drag_start_y = cursor_y;
                    drag_state.drag_start_scroll = state.scroll_offset;
                    break;
                }
            }
        }
    }

    // Handle dragging - directly update editor scroll offset
    if drag_state.is_dragging && mouse_button.pressed(MouseButton::Left) {
        if let Some(entity) = drag_state.dragging_entity {
            if let Ok((_, scrollbar)) = scrollbar_query.get(entity) {
                // Calculate how far the mouse has moved in world coordinates
                let delta_y = cursor_y - drag_state.drag_start_y;

                // Calculate thumb height and scrollable range
                let thumb_height = (scrollbar.visible_fraction * scrollbar.track_height).max(scrollbar.min_thumb_height);
                let scrollable_range = scrollbar.track_height - thumb_height;

                // Convert pixel movement to scroll offset change
                if scrollable_range > 0.0 {
                    // Calculate total scrollable content
                    let line_height = font.line_height;
                    let total_lines = state.line_count();
                    let total_content_height = total_lines as f32 * line_height;
                    let viewport_height = viewport.height as f32;
                    let max_scroll = -(total_content_height - viewport_height).max(0.0);

                    // Scale pixel delta to scroll offset
                    let scroll_delta = (delta_y / scrollable_range) * max_scroll;
                    let new_scroll_offset = (drag_state.drag_start_scroll + scroll_delta).clamp(max_scroll.min(0.0), 0.0);

                    // Only update target - the apply_scroll system will handle actual scroll update
                    // For scrollbar dragging, we want immediate response (no smoothing)
                    state.target_scroll_offset = new_scroll_offset;
                    state.needs_scroll_update = true;

                    // IMPORTANT: Update last_cursor_pos to prevent auto_scroll_to_cursor from
                    // snapping back after drag release. We keep the cursor at the same position
                    // so auto-scroll thinks the cursor hasn't moved.
                    state.last_cursor_pos = state.cursor_pos;
                }
            }
        }
    }

    // Handle mouse release
    if mouse_button.just_released(MouseButton::Left) {
        drag_state.is_dragging = false;
        drag_state.dragging_entity = None;
    }

    // Handle clicking on track (not implemented yet - could add jump-to-position behavior)
}

/// System that updates scrollbar visuals based on Scrollbar component
/// Also updates when editor state changes (for scroll position)
fn update_scrollbars(
    mut commands: Commands,
    scrollbar_query: Query<(Entity, &Scrollbar), Or<(Changed<Scrollbar>, With<EditorScrollbar>)>>,
    mut track_query: Query<(Entity, &ScrollbarTrack, &mut Transform, &mut Sprite, &mut Visibility)>,
    mut thumb_query: Query<(Entity, &ScrollbarThumb, &mut Transform, &mut Sprite, &mut Visibility), Without<ScrollbarTrack>>,
    state: Res<crate::types::CodeEditorState>,
    font: Res<crate::settings::FontSettings>,
    viewport: Res<crate::types::ViewportDimensions>,
    drag_state: Res<ScrollbarDragState>,
    mut last_scroll: Local<f32>,
) {
    // Only update if scroll offset changed (but always update during drag for smooth thumb movement)
    let scroll_changed = (*last_scroll - state.scroll_offset).abs() >= 0.01;
    if !scroll_changed && !drag_state.is_dragging && scrollbar_query.iter().count() > 0 {
        return;
    }
    if scroll_changed {
        *last_scroll = state.scroll_offset;
    }

    for (scrollbar_entity, scrollbar) in scrollbar_query.iter() {
        if !scrollbar.enabled {
            // Hide track and thumb for this scrollbar
            for (_, track, _, _, mut visibility) in track_query.iter_mut() {
                if track.parent == scrollbar_entity {
                    *visibility = Visibility::Hidden;
                }
            }
            for (_, thumb, _, _, mut visibility) in thumb_query.iter_mut() {
                if thumb.parent == scrollbar_entity {
                    *visibility = Visibility::Hidden;
                }
            }
            continue;
        }

        // Calculate scroll progress from editor state
        let line_height = font.line_height;
        let total_lines = state.line_count();
        let total_content_height = total_lines as f32 * line_height;
        let viewport_height = viewport.height as f32;
        let max_scroll = (total_content_height - viewport_height).max(0.0);

        let scroll_progress = if max_scroll > 0.0 {
            (-state.scroll_offset / max_scroll).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // Calculate thumb dimensions
        let thumb_height = (scrollbar.visible_fraction * scrollbar.track_height).max(scrollbar.min_thumb_height);
        let scrollable_range = scrollbar.track_height - thumb_height;
        let thumb_offset = scroll_progress * scrollable_range;

        // Track Y position (centered)
        let track_y = scrollbar.y;

        // Thumb Y position (from top)
        let thumb_y = scrollbar.y + scrollbar.track_height / 2.0 - thumb_offset - thumb_height / 2.0;

        // Find or create track
        let mut track_found = false;
        for (_, track, mut transform, mut sprite, mut visibility) in track_query.iter_mut() {
            if track.parent == scrollbar_entity {
                track_found = true;
                sprite.custom_size = Some(Vec2::new(scrollbar.width, scrollbar.track_height));
                sprite.color = scrollbar.track_color;
                transform.translation = Vec3::new(scrollbar.x, track_y, scrollbar.z_index);
                *visibility = Visibility::Visible;
                break;
            }
        }

        if !track_found {
            commands.spawn((
                Sprite {
                    color: scrollbar.track_color,
                    custom_size: Some(Vec2::new(scrollbar.width, scrollbar.track_height)),
                    ..default()
                },
                Transform::from_translation(Vec3::new(scrollbar.x, track_y, scrollbar.z_index)),
                ScrollbarTrack { parent: scrollbar_entity },
                Name::new(format!("ScrollbarTrack_{:?}", scrollbar_entity)),
                Visibility::Visible,
            ));
        }

        // Find or create thumb
        let mut thumb_found = false;
        for (_, thumb, mut transform, mut sprite, mut visibility) in thumb_query.iter_mut() {
            if thumb.parent == scrollbar_entity {
                thumb_found = true;
                sprite.custom_size = Some(Vec2::new(scrollbar.width, thumb_height));
                sprite.color = scrollbar.thumb_color;
                transform.translation = Vec3::new(scrollbar.x, thumb_y, scrollbar.z_index + 0.1);
                *visibility = Visibility::Visible;
                break;
            }
        }

        if !thumb_found {
            commands.spawn((
                Sprite {
                    color: scrollbar.thumb_color,
                    custom_size: Some(Vec2::new(scrollbar.width, thumb_height)),
                    ..default()
                },
                Transform::from_translation(Vec3::new(scrollbar.x, thumb_y, scrollbar.z_index + 0.1)),
                ScrollbarThumb { parent: scrollbar_entity },
                Name::new(format!("ScrollbarThumb_{:?}", scrollbar_entity)),
                Visibility::Visible,
            ));
        }
    }
}

/// Marker for the main editor scrollbar
#[derive(Component)]
pub struct EditorScrollbar;

/// Update the editor scrollbar based on editor state
pub fn update_editor_scrollbar(
    mut commands: Commands,
    state: Res<crate::types::CodeEditorState>,
    font: Res<crate::settings::FontSettings>,
    viewport: Res<crate::types::ViewportDimensions>,
    scrollbar_settings: Res<crate::settings::ScrollbarSettings>,
    mut scrollbar_query: Query<&mut Scrollbar, With<EditorScrollbar>>,
) {
    if !scrollbar_settings.enabled {
        // Hide scrollbar if disabled
        for mut scrollbar in scrollbar_query.iter_mut() {
            scrollbar.enabled = false;
        }
        return;
    }

    let viewport_height = viewport.height as f32;
    let viewport_width = viewport.width as f32;
    let line_height = font.line_height;
    let total_lines = state.line_count();
    let total_content_height = total_lines as f32 * line_height;

    // Scrollbar position (always at right edge)
    let scrollbar_x = viewport_width / 2.0 - scrollbar_settings.width / 2.0;

    if let Ok(mut scrollbar) = scrollbar_query.single_mut() {
        // Update existing scrollbar (scroll position is read from editor state in update_scrollbars)
        scrollbar.enabled = total_content_height > viewport_height;
        scrollbar.x = scrollbar_x;
        scrollbar.y = 0.0;
        scrollbar.width = scrollbar_settings.width;
        scrollbar.track_height = viewport_height;
        scrollbar.visible_fraction = (viewport_height / total_content_height).min(1.0);
        scrollbar.min_thumb_height = 30.0;
        scrollbar.z_index = 10.0;
        scrollbar.track_color = scrollbar_settings.background_color;
        scrollbar.thumb_color = scrollbar_settings.thumb_color;
        scrollbar.border_radius = 3.0;
    } else {
        // Create new scrollbar entity
        commands.spawn((
            Scrollbar {
                x: scrollbar_x,
                y: 0.0,
                width: scrollbar_settings.width,
                track_height: viewport_height,
                visible_fraction: (viewport_height / total_content_height).min(1.0),
                min_thumb_height: 30.0,
                z_index: 10.0,
                track_color: scrollbar_settings.background_color,
                thumb_color: scrollbar_settings.thumb_color,
                enabled: total_content_height > viewport_height,
                border_radius: 3.0,
            },
            EditorScrollbar,
            Name::new("EditorScrollbar"),
        ));
    }
}

