//! Generic, picking-driven scrollbar widget.
//!
//! Spawn a [`Scrollbar`] entity with a [`ScrollbarState`]; the plugin spawns
//! the track and thumb as child entities (sprites + [`Pickable`]) and
//! drives drag through `bevy_picking` observers. No window-coordinate
//! math, no global drag resource — each scrollbar entity holds its own
//! [`ScrollbarDragState`].
//!
//! ```rust,ignore
//! use bevy::prelude::*;
//! use bevy_text_engine::ui::{ScrollbarPlugin, Scrollbar, ScrollbarState};
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(ScrollbarPlugin)
//!     .add_systems(Startup, |mut commands: Commands| {
//!         commands.spawn((
//!             Scrollbar::default(),
//!             ScrollbarState::new(2000.0, 600.0),
//!         ));
//!     });
//! ```

use bevy::picking::pointer::PointerButton;
use bevy::picking::Pickable;
use bevy::prelude::*;

/// Plugin that registers types and the visual-update system. Drag is
/// driven by per-entity observers (no global system).
pub struct ScrollbarPlugin;

impl Plugin for ScrollbarPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Scrollbar>()
            .register_type::<ScrollbarState>()
            .register_type::<ScrollbarOrientation>()
            .register_type::<ScrollbarDragState>()
            .register_type::<ScrollbarTrack>()
            .register_type::<ScrollbarThumb>();

        app.add_systems(Update, update_scrollbars);
    }
}

/// How much content fits in how big a viewport, and where we are in it.
#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component, Default)]
pub struct ScrollbarState {
    pub content_size: f32,
    pub viewport_size: f32,
    /// 0 = top/left; negative = scrolled down/right.
    pub scroll_offset: f32,
    /// Smooth-scroll target. Animation loop should ease `scroll_offset` toward this.
    pub target_scroll_offset: f32,
}

impl Default for ScrollbarState {
    fn default() -> Self {
        Self {
            content_size: 100.0,
            viewport_size: 100.0,
            scroll_offset: 0.0,
            target_scroll_offset: 0.0,
        }
    }
}

impl ScrollbarState {
    pub fn new(content_size: f32, viewport_size: f32) -> Self {
        Self {
            content_size,
            viewport_size,
            scroll_offset: 0.0,
            target_scroll_offset: 0.0,
        }
    }

    /// Always <= 0.
    pub fn max_scroll(&self) -> f32 {
        -(self.content_size - self.viewport_size).max(0.0)
    }

    /// 0.0 = top, 1.0 = bottom.
    pub fn scroll_progress(&self) -> f32 {
        let max = self.max_scroll();
        if max >= 0.0 {
            0.0
        } else {
            (-self.scroll_offset / -max).clamp(0.0, 1.0)
        }
    }

    pub fn needs_scrollbar(&self) -> bool {
        self.content_size > self.viewport_size
    }

    pub fn set_scroll(&mut self, offset: f32) {
        self.scroll_offset = offset.clamp(self.max_scroll(), 0.0);
        self.target_scroll_offset = self.scroll_offset;
    }

    pub fn scroll_by(&mut self, delta: f32) {
        self.set_scroll(self.scroll_offset + delta);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum ScrollbarOrientation {
    #[default]
    Vertical,
    Horizontal,
}

/// Scrollbar configuration + visual style. Cascades [`ScrollbarDragState`].
#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component, Default)]
#[require(ScrollbarDragState)]
pub struct Scrollbar {
    pub orientation: ScrollbarOrientation,
    pub x: f32,
    pub y: f32,
    /// Thickness (perpendicular to scroll axis).
    pub width: f32,
    pub track_length: f32,
    pub min_thumb_size: f32,
    pub z_index: f32,
    pub track_color: Color,
    pub thumb_color: Color,
    pub thumb_hover_color: Color,
    pub enabled: bool,
    /// Render layer to put track + thumb on. Inherited by children.
    #[reflect(ignore)]
    pub render_layers: Option<bevy_camera::visibility::RenderLayers>,
}

impl Default for Scrollbar {
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

/// Track sprite child. The `parent` field carries the [`Scrollbar`] entity
/// so picking observers can route back to it.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct ScrollbarTrack {
    pub parent: Entity,
}

/// Thumb sprite child. Holds [`Pickable`]; drag observers fire on it.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct ScrollbarThumb {
    pub parent: Entity,
}

/// Per-scrollbar drag state. Cascaded onto every [`Scrollbar`] via
/// `#[require]` — no global drag resource.
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
pub struct ScrollbarDragState {
    pub is_dragging: bool,
    /// Pointer position when drag started, in the same axis the scrollbar tracks.
    pub drag_start_pointer: f32,
    pub drag_start_scroll: f32,
}

/// Picking observer wired by [`update_scrollbars`] when the thumb is spawned.
fn on_thumb_press(
    trigger: On<Pointer<Press>>,
    thumbs: Query<&ScrollbarThumb>,
    mut bars: Query<(&Scrollbar, &ScrollbarState, &mut ScrollbarDragState)>,
) {
    if trigger.event().button != PointerButton::Primary {
        return;
    }
    let Ok(thumb) = thumbs.get(trigger.event().entity) else {
        return;
    };
    let Ok((bar, state, mut drag)) = bars.get_mut(thumb.parent) else {
        return;
    };
    let pointer = pointer_axis(bar.orientation, trigger.event().pointer_location.position);
    drag.is_dragging = true;
    drag.drag_start_pointer = pointer;
    drag.drag_start_scroll = state.scroll_offset;
}

fn on_thumb_drag(
    trigger: On<Pointer<Drag>>,
    thumbs: Query<&ScrollbarThumb>,
    mut bars: Query<(&Scrollbar, &mut ScrollbarState, &ScrollbarDragState)>,
) {
    if trigger.event().button != PointerButton::Primary {
        return;
    }
    let Ok(thumb) = thumbs.get(trigger.event().entity) else {
        return;
    };
    let Ok((bar, mut state, drag)) = bars.get_mut(thumb.parent) else {
        return;
    };
    if !drag.is_dragging {
        return;
    }
    let pointer = pointer_axis(bar.orientation, trigger.event().pointer_location.position);
    // Window y-up vs world y-down: invert vertical pointer delta so dragging
    // the thumb down scrolls content down.
    let delta = match bar.orientation {
        ScrollbarOrientation::Vertical => drag.drag_start_pointer - pointer,
        ScrollbarOrientation::Horizontal => pointer - drag.drag_start_pointer,
    };

    let visible_fraction = (state.viewport_size / state.content_size).min(1.0);
    let thumb_size = (visible_fraction * bar.track_length).max(bar.min_thumb_size);
    let scrollable_range = bar.track_length - thumb_size;
    if scrollable_range <= 0.0 {
        return;
    }

    let max_scroll = state.max_scroll();
    let scroll_delta = (delta / scrollable_range) * max_scroll;
    let new_scroll = (drag.drag_start_scroll + scroll_delta).clamp(max_scroll, 0.0);
    state.scroll_offset = new_scroll;
    state.target_scroll_offset = new_scroll;
}

fn on_thumb_release(
    trigger: On<Pointer<Release>>,
    thumbs: Query<&ScrollbarThumb>,
    mut bars: Query<&mut ScrollbarDragState>,
) {
    if trigger.event().button != PointerButton::Primary {
        return;
    }
    let Ok(thumb) = thumbs.get(trigger.event().entity) else {
        return;
    };
    if let Ok(mut drag) = bars.get_mut(thumb.parent) {
        drag.is_dragging = false;
    }
}

fn pointer_axis(orientation: ScrollbarOrientation, pos: Vec2) -> f32 {
    match orientation {
        ScrollbarOrientation::Vertical => pos.y,
        ScrollbarOrientation::Horizontal => pos.x,
    }
}

/// Spawn-on-first-frame and per-frame transform/visibility update for the
/// track and thumb children. Picking observers attached to the thumb on
/// creation handle drag.
#[allow(clippy::type_complexity)]
fn update_scrollbars(
    mut commands: Commands,
    bars: Query<(Entity, &Scrollbar, &ScrollbarState)>,
    mut tracks: Query<(
        &ScrollbarTrack,
        &mut Transform,
        &mut Sprite,
        &mut Visibility,
    )>,
    mut thumbs: Query<
        (
            &ScrollbarThumb,
            &mut Transform,
            &mut Sprite,
            &mut Visibility,
        ),
        Without<ScrollbarTrack>,
    >,
) {
    for (bar_entity, bar, state) in bars.iter() {
        let visible = bar.enabled && state.needs_scrollbar();

        if !visible {
            for (track, _, _, mut vis) in tracks.iter_mut() {
                if track.parent == bar_entity {
                    *vis = Visibility::Hidden;
                }
            }
            for (thumb, _, _, mut vis) in thumbs.iter_mut() {
                if thumb.parent == bar_entity {
                    *vis = Visibility::Hidden;
                }
            }
            continue;
        }

        let visible_fraction = (state.viewport_size / state.content_size).min(1.0);
        let thumb_size = (visible_fraction * bar.track_length).max(bar.min_thumb_size);
        let scrollable_range = bar.track_length - thumb_size;
        let progress = state.scroll_progress();
        let thumb_offset = progress * scrollable_range;

        let (track_size, thumb_actual_size, track_pos, thumb_pos) = match bar.orientation {
            ScrollbarOrientation::Vertical => {
                let track_size = Vec2::new(bar.width, bar.track_length);
                let thumb_size_vec = Vec2::new(bar.width, thumb_size);
                let track_pos = Vec3::new(bar.x, bar.y, bar.z_index);
                let thumb_y = bar.y + bar.track_length / 2.0 - thumb_offset - thumb_size / 2.0;
                let thumb_pos = Vec3::new(bar.x, thumb_y, bar.z_index + 0.1);
                (track_size, thumb_size_vec, track_pos, thumb_pos)
            }
            ScrollbarOrientation::Horizontal => {
                let track_size = Vec2::new(bar.track_length, bar.width);
                let thumb_size_vec = Vec2::new(thumb_size, bar.width);
                let track_pos = Vec3::new(bar.x, bar.y, bar.z_index);
                let thumb_x = bar.x - bar.track_length / 2.0 + thumb_offset + thumb_size / 2.0;
                let thumb_pos = Vec3::new(thumb_x, bar.y, bar.z_index + 0.1);
                (track_size, thumb_size_vec, track_pos, thumb_pos)
            }
        };

        let mut track_found = false;
        for (track, mut transform, mut sprite, mut vis) in tracks.iter_mut() {
            if track.parent == bar_entity {
                track_found = true;
                sprite.custom_size = Some(track_size);
                sprite.color = bar.track_color;
                transform.translation = track_pos;
                *vis = Visibility::Visible;
                break;
            }
        }
        if !track_found {
            let mut e = commands.spawn((
                Sprite {
                    color: bar.track_color,
                    custom_size: Some(track_size),
                    ..default()
                },
                Transform::from_translation(track_pos),
                ScrollbarTrack { parent: bar_entity },
                ChildOf(bar_entity),
                Pickable::IGNORE,
                Name::new(format!("ScrollbarTrack_{:?}", bar_entity)),
                Visibility::Visible,
            ));
            if let Some(layers) = &bar.render_layers {
                e.insert(layers.clone());
            }
        }

        let mut thumb_found = false;
        for (thumb, mut transform, mut sprite, mut vis) in thumbs.iter_mut() {
            if thumb.parent == bar_entity {
                thumb_found = true;
                sprite.custom_size = Some(thumb_actual_size);
                sprite.color = bar.thumb_color;
                transform.translation = thumb_pos;
                *vis = Visibility::Visible;
                break;
            }
        }
        if !thumb_found {
            let mut e = commands.spawn((
                Sprite {
                    color: bar.thumb_color,
                    custom_size: Some(thumb_actual_size),
                    ..default()
                },
                Transform::from_translation(thumb_pos),
                ScrollbarThumb { parent: bar_entity },
                ChildOf(bar_entity),
                Pickable::default(),
                Name::new(format!("ScrollbarThumb_{:?}", bar_entity)),
                Visibility::Visible,
            ));
            e.observe(on_thumb_press)
                .observe(on_thumb_drag)
                .observe(on_thumb_release);
            if let Some(layers) = &bar.render_layers {
                e.insert(layers.clone());
            }
        }
    }
}
