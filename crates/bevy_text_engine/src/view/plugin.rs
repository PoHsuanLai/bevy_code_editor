//! Text view plugin — registers the rendering and scroll animation systems
//! that turn `TextView` entities into GPU draw batches.
//!
//! This module also defines [`TextEnginePlugins`], a [`PluginGroup`] that
//! bundles the GPU plugins from [`crate::gpu`] together with the view-side
//! [`TextEnginePlugin`]. Hosts that just want "render styled text" should
//! add `TextEnginePlugins`; those that already manage the GPU pipeline
//! themselves can add [`TextEnginePlugin`] alone.

use bevy::app::{PluginGroup, PluginGroupBuilder};
use bevy::prelude::*;

use super::font::FontConfig;
use super::layout::DisplayLayout;
use super::overlay::TextViewOverlays;
use super::render::{render_layout, GlyphBatchComponent, TextViewBatch};
use super::state::TextViewState;
use super::viewport::TextViewViewport;
use crate::gpu::{atlas_ready, GlyphAtlas, GlyphAtlasPlugin, InstancedTextRenderPlugin};

/// System set for text view rendering.
///
/// `update_text_views` runs in this set; downstream systems that read the
/// freshly-built `GlyphBatchComponent` can order themselves with
/// `.after(TextViewRenderSet)`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TextViewRenderSet;

/// Marker component for a text view rendered by [`TextEnginePlugin`].
///
/// `#[require]` cascades the rest of the rendering machinery, so spawning
/// `TextView` alone is enough to get a usable text-rendering entity
/// (mirror of `bevy_text::Text2d`).
///
/// `Pickable` is also cascaded so a custom `bevy_picking` backend (in
/// `bevy_text_interaction::picking`) can produce `PointerHits` for this
/// entity, routing pointer events / scroll / drag observers correctly. The
/// engine itself doesn't run the backend — adding `Pickable` is a no-op
/// without `TextInteractionPlugin` (or another picking backend) registered,
/// it just declares "this entity wants to be pickable."
///
/// This component intentionally requires only engine-side data. Editor /
/// interaction layers attach their own components via their own
/// `#[require]` cascades on top of `TextView`.
#[derive(Component, Default, Reflect)]
#[reflect(Component, Default)]
#[require(
    TextViewState,
    TextViewViewport,
    DisplayLayout,
    TextViewOverlays,
    FontConfig,
    bevy::picking::Pickable,
)]
pub struct TextView;

/// Component that links a text view to its batch rendering entity.
/// Managed automatically by [`update_text_views`].
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct TextViewBatchEntity(pub Entity);

/// View-side plugin: registers the rendering and scroll animation systems.
///
/// Does **not** add the GPU plugins — those live in [`crate::gpu`] and are
/// bundled into [`TextEnginePlugins`]. If you only need the systems
/// (because you've already added the GPU plugins manually), add this
/// directly. Most consumers should add [`TextEnginePlugins`] instead.
#[derive(Default)]
pub struct TextEnginePlugin;

impl Plugin for TextEnginePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<FontConfig>()
            .register_type::<super::overlay::RectOverlay>()
            .register_type::<super::overlay::RowVertical>()
            .register_type::<TextView>()
            .register_type::<TextViewBatchEntity>()
            .register_type::<TextViewOverlays>()
            .register_type::<TextViewState>()
            .register_type::<TextViewViewport>()
            .register_type::<super::viewport::ViewportOrigin>();

        app.add_systems(
            Update,
            (
                animate_text_view_scroll,
                update_text_views
                    .run_if(atlas_ready)
                    .in_set(TextViewRenderSet),
            )
                .chain(),
        );
    }
}

/// `PluginGroup` bundling everything needed to render `TextView` entities:
/// [`GlyphAtlasPlugin`] (atlas resource bootstrap),
/// [`InstancedTextRenderPlugin`] (instanced draw pipeline), and
/// [`TextEnginePlugin`] (view systems).
///
/// Mirror of `bevy::DefaultPlugins`: hosts that want fine-grained control
/// can `.disable::<X>()` individual plugins or build their own group.
pub struct TextEnginePlugins;

impl PluginGroup for TextEnginePlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(GlyphAtlasPlugin)
            .add(InstancedTextRenderPlugin)
            .add(TextEnginePlugin)
    }
}

/// Smooth scroll animation for text views.
fn animate_text_view_scroll(mut query: Query<&mut TextViewState, With<TextView>>, time: Res<Time>) {
    let dt = time.delta_secs();
    let lerp_speed = 12.0; // Exponential decay factor

    for mut state in query.iter_mut() {
        // Vertical scroll
        let diff_v = state.target_scroll_offset - state.scroll_offset;
        if diff_v.abs() > 0.5 {
            state.scroll_offset += diff_v * (1.0 - (-lerp_speed * dt).exp());
        } else if diff_v.abs() > 0.001 {
            state.scroll_offset = state.target_scroll_offset;
        }

        // Horizontal scroll
        let diff_h = state.target_horizontal_scroll_offset - state.horizontal_scroll_offset;
        if diff_h.abs() > 0.5 {
            state.horizontal_scroll_offset += diff_h * (1.0 - (-lerp_speed * dt).exp());
        } else if diff_h.abs() > 0.001 {
            state.horizontal_scroll_offset = state.target_horizontal_scroll_offset;
        }
    }
}

/// Main rendering system for all text views.
///
/// Queries entities with `TextView`, `TextViewState`, `TextViewViewport`, and
/// `DisplayLayout`, rendering each via `render_layout()`. Consumers must
/// provide a `DisplayLayout` (e.g. via `view::snapshot::trivial_layout` for
/// static content, or compute one from a display map).
#[allow(clippy::type_complexity)]
pub(crate) fn update_text_views(
    mut commands: Commands,
    mut text_views: Query<
        (
            Entity,
            &TextViewState,
            &TextViewViewport,
            &FontConfig,
            Ref<DisplayLayout>,
            Option<Ref<TextViewOverlays>>,
            Option<&TextViewBatchEntity>,
            Option<&bevy_camera::visibility::RenderLayers>,
        ),
        With<TextView>,
    >,
    mut atlas: ResMut<GlyphAtlas>,
    mut images: ResMut<Assets<Image>>,
    fonts: Res<Assets<bevy::text::Font>>,
) {
    for (tv_entity, state, viewport, font, layout, overlays, batch_entity_opt, render_layers) in
        text_views.iter_mut()
    {
        let font_id = font
            .font
            .as_ref()
            .and_then(|h| atlas.ensure_font(h, &fonts));
        // Skip-on-unchanged: if neither the display layout nor the overlays
        // changed since last frame, the GPU batch is still valid — skip the
        // rebuild + atlas upload entirely.
        let overlays_changed = overlays.as_ref().map(|o| o.is_changed()).unwrap_or(false);
        if !layout.is_changed() && !overlays_changed && batch_entity_opt.is_some() {
            continue;
        }
        let layout: &DisplayLayout = &layout;
        let overlays = overlays.as_deref();
        let content_start_x = if viewport.gutter_width > 0.0 {
            viewport.text_area_left.max(viewport.gutter_width)
        } else {
            viewport.text_area_left
        };

        let instances = render_layout(
            layout,
            overlays,
            viewport,
            &mut atlas,
            content_start_x,
            state.horizontal_scroll_offset,
            font.size,
            font_id,
        );

        atlas.update_texture(&mut images);

        let line_height = layout.line_height;
        let scroll_dist = state.scroll_offset.abs();
        let start_pixels = scroll_dist - viewport.text_area_top;
        let first_visible = (start_pixels / line_height).floor().max(0.0) as usize;
        let visible_count = ((viewport.height as f32) / line_height).ceil() as usize;
        let last_visible = first_visible + visible_count;

        let batch_data = TextViewBatch {
            built_at_scroll: state.scroll_offset,
            built_at_horizontal_scroll: state.horizontal_scroll_offset,
            first_line: first_visible,
            last_line: last_visible,
            built_at_width: viewport.width,
            built_at_height: viewport.height,
        };

        if instances.is_empty() {
            if let Some(batch_e) = batch_entity_opt {
                commands.entity(batch_e.0).insert(Visibility::Hidden);
            }
            continue;
        }

        let layer = render_layers.and_then(|l| {
            (0u8..=31)
                .find(|&i| l.intersects(&bevy_camera::visibility::RenderLayers::layer(i as usize)))
        });
        let batch_comp = GlyphBatchComponent {
            instances,
            atlas_texture: atlas.texture.clone(),
            render_layer: layer,
        };

        if let Some(batch_e) = batch_entity_opt {
            let mut cmds = commands.entity(batch_e.0);
            cmds.insert(batch_comp)
                .insert(Visibility::Visible)
                .insert(batch_data);
            if let Some(layers) = render_layers {
                cmds.insert(layers.clone());
            }
        } else {
            let mut entity_cmds = commands.spawn((
                batch_comp,
                Transform::default(),
                GlobalTransform::default(),
                batch_data,
                Name::new("TextViewBatch"),
                Visibility::Visible,
                InheritedVisibility::default(),
                ViewVisibility::default(),
            ));
            if let Some(layers) = render_layers {
                entity_cmds.insert(layers.clone());
            }
            let batch_entity = entity_cmds.id();
            commands
                .entity(tv_entity)
                .insert(TextViewBatchEntity(batch_entity));
        }
    }
}
