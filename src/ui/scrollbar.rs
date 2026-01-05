//! Generic Scrollbar UI Component
//!
//! A reusable scrollbar that works with any scrollable content.
//! Not tied to CodeEditorState - uses a generic ScrollState component.
//!
//! # Usage
//!
//! ```rust,ignore
//! use bevy::prelude::*;
//! use bevy_code_editor::ui::{GenericScrollbarPlugin, GenericScrollbar, ScrollState};
//!
//! fn main() {
//!     App::new()
//!         .add_plugins(DefaultPlugins)
//!         .add_plugins(GenericScrollbarPlugin)
//!         .add_systems(Startup, setup)
//!         .run();
//! }
//!
//! fn setup(mut commands: Commands) {
//!     // Create a scrollable area with a scrollbar
//!     commands.spawn((
//!         GenericScrollbar::default(),
//!         ScrollState {
//!             content_size: 2000.0,
//!             viewport_size: 600.0,
//!             scroll_offset: 0.0,
//!         },
//!     ));
//! }
//! ```

use bevy::prelude::*;

/// Plugin for the generic scrollbar system
pub struct GenericScrollbarPlugin;

impl Plugin for GenericScrollbarPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GenericScrollbarDragState::default())
            .add_systems(
                Update,
                (handle_generic_scrollbar_input, update_generic_scrollbars).chain(),
            );
    }
}

/// Scroll state for a scrollable area
/// Attach this component alongside GenericScrollbar to control it
#[derive(Component, Clone, Debug)]
pub struct ScrollState {
    /// Total content size (height for vertical, width for horizontal)
    pub content_size: f32,
    /// Visible viewport size
    pub viewport_size: f32,
    /// Current scroll offset (0 = top/left, negative = scrolled down/right)
    pub scroll_offset: f32,
    /// Target scroll offset for smooth scrolling (optional)
    pub target_scroll_offset: f32,
}

impl Default for ScrollState {
    fn default() -> Self {
        Self {
            content_size: 100.0,
            viewport_size: 100.0,
            scroll_offset: 0.0,
            target_scroll_offset: 0.0,
        }
    }
}

impl ScrollState {
    /// Create a new scroll state
    pub fn new(content_size: f32, viewport_size: f32) -> Self {
        Self {
            content_size,
            viewport_size,
            scroll_offset: 0.0,
            target_scroll_offset: 0.0,
        }
    }

    /// Get the maximum scroll offset (always <= 0)
    pub fn max_scroll(&self) -> f32 {
        -(self.content_size - self.viewport_size).max(0.0)
    }

    /// Get scroll progress (0.0 = top, 1.0 = bottom)
    pub fn scroll_progress(&self) -> f32 {
        let max = self.max_scroll();
        if max >= 0.0 {
            0.0
        } else {
            (-self.scroll_offset / -max).clamp(0.0, 1.0)
        }
    }

    /// Check if scrolling is needed
    pub fn needs_scrollbar(&self) -> bool {
        self.content_size > self.viewport_size
    }

    /// Set scroll offset (clamped to valid range)
    pub fn set_scroll(&mut self, offset: f32) {
        self.scroll_offset = offset.clamp(self.max_scroll(), 0.0);
        self.target_scroll_offset = self.scroll_offset;
    }

    /// Scroll by a delta (positive = scroll up, negative = scroll down)
    pub fn scroll_by(&mut self, delta: f32) {
        self.set_scroll(self.scroll_offset + delta);
    }
}

/// Generic scrollbar component
/// Attach this to an entity to create a scrollbar
#[derive(Component, Clone, Debug)]
pub struct GenericScrollbar {
    /// Orientation (vertical or horizontal)
    pub orientation: ScrollbarOrientation,
    /// X position in world coordinates
    pub x: f32,
    /// Y position (center)
    pub y: f32,
    /// Width of the scrollbar (thickness for vertical)
    pub width: f32,
    /// Length of the scrollbar track
    pub track_length: f32,
    /// Minimum thumb size in pixels
    pub min_thumb_size: f32,
    /// Z-index for rendering
    pub z_index: f32,
    /// Track color
    pub track_color: Color,
    /// Thumb color
    pub thumb_color: Color,
    /// Thumb hover color
    pub thumb_hover_color: Color,
    /// Whether the scrollbar is enabled
    pub enabled: bool,
    /// Render layers (optional)
    pub render_layers: Option<bevy_camera::visibility::RenderLayers>,
}

impl Default for GenericScrollbar {
    fn default() -> Self {
        Self {
            orientation: ScrollbarOrientation::Vertical,
            x: 0.0,
            y: 0.0,
            width: 12.0,
            track_length: 600.0,
            min_thumb_size: 30.0,
            z_index: 100.0,
            track_color: Color::srgba(0.2, 0.2, 0.2, 0.3),
            thumb_color: Color::srgba(0.5, 0.5, 0.5, 0.5),
            thumb_hover_color: Color::srgba(0.6, 0.6, 0.6, 0.7),
            enabled: true,
            render_layers: None,
        }
    }
}

/// Scrollbar orientation
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollbarOrientation {
    #[default]
    Vertical,
    Horizontal,
}

/// Marker for scrollbar track entity
#[derive(Component)]
pub struct GenericScrollbarTrack {
    pub parent: Entity,
}

/// Marker for scrollbar thumb entity
#[derive(Component)]
pub struct GenericScrollbarThumb {
    pub parent: Entity,
}

/// Resource to track scrollbar drag state
#[derive(Resource, Default)]
pub struct GenericScrollbarDragState {
    /// Whether we're currently dragging a scrollbar
    pub is_dragging: bool,
    /// The entity of the scrollbar being dragged
    pub dragging_entity: Option<Entity>,
    /// Initial mouse position when drag started
    pub drag_start_pos: f32,
    /// Initial scroll offset when drag started
    pub drag_start_scroll: f32,
}

/// Handle mouse input for scrollbar interaction
fn handle_generic_scrollbar_input(
    windows: Query<&Window>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut drag_state: ResMut<GenericScrollbarDragState>,
    mut scrollbar_query: Query<(Entity, &GenericScrollbar, &mut ScrollState)>,
    thumb_query: Query<(&GenericScrollbarThumb, &Transform, &Sprite)>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_pos_window) = window.cursor_position() else {
        if mouse_button.just_released(MouseButton::Left) {
            drag_state.is_dragging = false;
            drag_state.dragging_entity = None;
        }
        return;
    };

    // Convert to world coordinates
    let cursor_x = cursor_pos_window.x - window.width() / 2.0;
    let cursor_y = cursor_pos_window.y - window.height() / 2.0;

    // Handle mouse button just pressed
    if mouse_button.just_pressed(MouseButton::Left) {
        for (thumb, transform, sprite) in thumb_query.iter() {
            let Some(size) = sprite.custom_size else {
                continue;
            };
            let thumb_x = transform.translation.x;
            let thumb_y = transform.translation.y;
            let thumb_half_width = size.x / 2.0;
            let thumb_half_height = size.y / 2.0;

            if cursor_x >= thumb_x - thumb_half_width
                && cursor_x <= thumb_x + thumb_half_width
                && cursor_y >= thumb_y - thumb_half_height
                && cursor_y <= thumb_y + thumb_half_height
            {
                if let Ok((entity, scrollbar, scroll_state)) = scrollbar_query.get(thumb.parent) {
                    drag_state.is_dragging = true;
                    drag_state.dragging_entity = Some(entity);
                    drag_state.drag_start_pos = match scrollbar.orientation {
                        ScrollbarOrientation::Vertical => cursor_y,
                        ScrollbarOrientation::Horizontal => cursor_x,
                    };
                    drag_state.drag_start_scroll = scroll_state.scroll_offset;
                    break;
                }
            }
        }
    }

    // Handle dragging
    if drag_state.is_dragging && mouse_button.pressed(MouseButton::Left) {
        if let Some(entity) = drag_state.dragging_entity {
            if let Ok((_, scrollbar, mut scroll_state)) = scrollbar_query.get_mut(entity) {
                let current_pos = match scrollbar.orientation {
                    ScrollbarOrientation::Vertical => cursor_y,
                    ScrollbarOrientation::Horizontal => cursor_x,
                };
                let delta = current_pos - drag_state.drag_start_pos;

                // Calculate thumb size
                let visible_fraction =
                    (scroll_state.viewport_size / scroll_state.content_size).min(1.0);
                let thumb_size =
                    (visible_fraction * scrollbar.track_length).max(scrollbar.min_thumb_size);
                let scrollable_range = scrollbar.track_length - thumb_size;

                if scrollable_range > 0.0 {
                    let max_scroll = scroll_state.max_scroll();
                    let scroll_delta = (delta / scrollable_range) * max_scroll;
                    let new_scroll =
                        (drag_state.drag_start_scroll + scroll_delta).clamp(max_scroll, 0.0);
                    scroll_state.scroll_offset = new_scroll;
                    scroll_state.target_scroll_offset = new_scroll;
                }
            }
        }
    }

    // Handle mouse release
    if mouse_button.just_released(MouseButton::Left) {
        drag_state.is_dragging = false;
        drag_state.dragging_entity = None;
    }
}

/// Update scrollbar visuals
fn update_generic_scrollbars(
    mut commands: Commands,
    scrollbar_query: Query<(Entity, &GenericScrollbar, &ScrollState)>,
    mut track_query: Query<(
        Entity,
        &GenericScrollbarTrack,
        &mut Transform,
        &mut Sprite,
        &mut Visibility,
    )>,
    mut thumb_query: Query<
        (
            Entity,
            &GenericScrollbarThumb,
            &mut Transform,
            &mut Sprite,
            &mut Visibility,
        ),
        Without<GenericScrollbarTrack>,
    >,
) {
    for (scrollbar_entity, scrollbar, scroll_state) in scrollbar_query.iter() {
        if !scrollbar.enabled || !scroll_state.needs_scrollbar() {
            // Hide track and thumb
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
        let visible_fraction = (scroll_state.viewport_size / scroll_state.content_size).min(1.0);
        let thumb_size = (visible_fraction * scrollbar.track_length).max(scrollbar.min_thumb_size);
        let scrollable_range = scrollbar.track_length - thumb_size;
        let scroll_progress = scroll_state.scroll_progress();
        let thumb_offset = scroll_progress * scrollable_range;

        // Calculate positions based on orientation
        let (track_size, thumb_actual_size, track_pos, thumb_pos) = match scrollbar.orientation {
            ScrollbarOrientation::Vertical => {
                let track_size = Vec2::new(scrollbar.width, scrollbar.track_length);
                let thumb_size_vec = Vec2::new(scrollbar.width, thumb_size);
                let track_pos = Vec3::new(scrollbar.x, scrollbar.y, scrollbar.z_index);
                let thumb_y =
                    scrollbar.y + scrollbar.track_length / 2.0 - thumb_offset - thumb_size / 2.0;
                let thumb_pos = Vec3::new(scrollbar.x, thumb_y, scrollbar.z_index + 0.1);
                (track_size, thumb_size_vec, track_pos, thumb_pos)
            }
            ScrollbarOrientation::Horizontal => {
                let track_size = Vec2::new(scrollbar.track_length, scrollbar.width);
                let thumb_size_vec = Vec2::new(thumb_size, scrollbar.width);
                let track_pos = Vec3::new(scrollbar.x, scrollbar.y, scrollbar.z_index);
                let thumb_x =
                    scrollbar.x - scrollbar.track_length / 2.0 + thumb_offset + thumb_size / 2.0;
                let thumb_pos = Vec3::new(thumb_x, scrollbar.y, scrollbar.z_index + 0.1);
                (track_size, thumb_size_vec, track_pos, thumb_pos)
            }
        };

        // Update or create track
        let mut track_found = false;
        for (_, track, mut transform, mut sprite, mut visibility) in track_query.iter_mut() {
            if track.parent == scrollbar_entity {
                track_found = true;
                sprite.custom_size = Some(track_size);
                sprite.color = scrollbar.track_color;
                transform.translation = track_pos;
                *visibility = Visibility::Visible;
                break;
            }
        }

        if !track_found {
            let mut entity_cmd = commands.spawn((
                Sprite {
                    color: scrollbar.track_color,
                    custom_size: Some(track_size),
                    ..default()
                },
                Transform::from_translation(track_pos),
                GenericScrollbarTrack {
                    parent: scrollbar_entity,
                },
                Name::new(format!("GenericScrollbarTrack_{:?}", scrollbar_entity)),
                Visibility::Visible,
            ));
            if let Some(ref layers) = scrollbar.render_layers {
                entity_cmd.insert(layers.clone());
            }
        }

        // Update or create thumb
        let mut thumb_found = false;
        for (_, thumb, mut transform, mut sprite, mut visibility) in thumb_query.iter_mut() {
            if thumb.parent == scrollbar_entity {
                thumb_found = true;
                sprite.custom_size = Some(thumb_actual_size);
                sprite.color = scrollbar.thumb_color;
                transform.translation = thumb_pos;
                *visibility = Visibility::Visible;
                break;
            }
        }

        if !thumb_found {
            let mut entity_cmd = commands.spawn((
                Sprite {
                    color: scrollbar.thumb_color,
                    custom_size: Some(thumb_actual_size),
                    ..default()
                },
                Transform::from_translation(thumb_pos),
                GenericScrollbarThumb {
                    parent: scrollbar_entity,
                },
                Name::new(format!("GenericScrollbarThumb_{:?}", scrollbar_entity)),
                Visibility::Visible,
            ));
            if let Some(ref layers) = scrollbar.render_layers {
                entity_cmd.insert(layers.clone());
            }
        }
    }
}
