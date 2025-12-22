//! Standalone scrollbar plugin
//!
//! Provides a reusable scrollbar component that can be added to any entity

use bevy::prelude::*;

/// Scrollbar plugin - manages scrollbar rendering and interaction
pub struct ScrollbarPlugin;

impl Plugin for ScrollbarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, update_scrollbars);
    }
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
    /// Scroll progress (0.0 = top, 1.0 = bottom)
    pub scroll_progress: f32,
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
            scroll_progress: 0.0,
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
struct ScrollbarTrack {
    parent: Entity,
}

/// Marker for scrollbar thumb entity
#[derive(Component)]
struct ScrollbarThumb {
    parent: Entity,
}

/// System that updates scrollbar visuals based on Scrollbar component
fn update_scrollbars(
    mut commands: Commands,
    scrollbar_query: Query<(Entity, &Scrollbar), Changed<Scrollbar>>,
    mut track_query: Query<(Entity, &ScrollbarTrack, &mut Transform, &mut Sprite, &mut Visibility)>,
    mut thumb_query: Query<(Entity, &ScrollbarThumb, &mut Transform, &mut Sprite, &mut Visibility), Without<ScrollbarTrack>>,
) {
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

        // Calculate thumb dimensions
        let thumb_height = (scrollbar.visible_fraction * scrollbar.track_height).max(scrollbar.min_thumb_height);
        let scrollable_range = scrollbar.track_height - thumb_height;
        let thumb_offset = scrollbar.scroll_progress * scrollable_range;

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
