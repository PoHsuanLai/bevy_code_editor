//! TextViewPlugin — standalone Bevy plugin for rendering text views.
//!
//! This plugin provides everything needed to render styled text in a scrollable viewport
//! without any editor-specific concerns. It can be used independently for chat panels,
//! log viewers, or any other text display.

use bevy::prelude::*;

use super::layout::DisplayLayout;
use super::overlay::TextViewOverlays;
use super::render::{render_layout, GlyphBatchComponent, TextViewBatch};
use super::state::TextViewState;
use super::viewport::TextViewViewport;
use crate::gpu_text::GlyphAtlas;
use crate::settings::FontSettings;

/// System set for text view rendering
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TextViewRenderSet;

/// Standalone plugin for rendering text views.
///
/// This plugin:
/// - Registers the GPU text rendering infrastructure (atlas, instanced rendering)
/// - Runs systems to render all entities with `TextViewState` + `TextViewViewport`
/// - Handles debouncing and smooth scrolling for all text views
///
/// ## Usage
///
/// ```rust,ignore
/// app.add_plugins(TextViewPlugin);
///
/// // Spawn a text view
/// commands.spawn((
///     TextViewState::with_text("Hello!"),
///     TextViewViewport::default(),
/// ));
/// ```
pub struct TextViewPlugin;

impl Plugin for TextViewPlugin {
    fn build(&self, app: &mut App) {
        // Add GPU rendering infrastructure (skip if already registered by CodeEditorPlugin)
        if !app.is_plugin_added::<crate::gpu_text::GpuTextPlugin>() {
            app.add_plugins(crate::gpu_text::GpuTextPlugin);
        }
        if !app.is_plugin_added::<crate::gpu_text::InstancedTextRenderPlugin>() {
            app.add_plugins(crate::gpu_text::InstancedTextRenderPlugin);
        }

        // Interaction resources
        app.init_resource::<super::interaction::TextViewDragState>();

        // Text view systems
        app.add_systems(
            Update,
            (
                animate_text_view_scroll,
                update_text_views
                    .run_if(crate::gpu_text::atlas_ready)
                    .in_set(TextViewRenderSet),
            )
                .chain(),
        );

        // Interaction systems (scroll, selection, copy)
        app.add_systems(
            Update,
            (
                super::interaction::handle_text_view_scroll,
                super::interaction::handle_text_view_mouse,
                super::interaction::handle_text_view_copy,
            ),
        );
    }
}

/// Marker component to identify text views managed by TextViewPlugin.
///
/// Entities with this marker (plus `TextViewState` + `TextViewViewport`)
/// will be rendered by the `update_text_views` system.
#[derive(Component, Default)]
pub struct TextView;

/// Component that links a text view to its batch rendering entity.
/// Managed automatically by `update_text_views`.
#[derive(Component)]
pub struct TextViewBatchEntity(pub Entity);

/// Smooth scroll animation for text views
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
/// `DisplayLayout`, rendering each via `render_layout()`. The legacy
/// `render_text_view` path is gone; consumers must provide a `DisplayLayout`,
/// e.g. via `text_view::trivial_layout` for static content.
///
/// Exposed `pub(crate)` so the editor plugin can register it directly without
/// adding `TextViewPlugin` (which would double-add scroll/input systems the
/// editor handles itself).
#[allow(clippy::type_complexity)]
pub(crate) fn update_text_views(
    mut commands: Commands,
    mut text_views: Query<
        (
            Entity,
            &TextViewState,
            &TextViewViewport,
            Ref<DisplayLayout>,
            Option<Ref<TextViewOverlays>>,
            Option<&TextViewBatchEntity>,
            Option<&bevy_camera::visibility::RenderLayers>,
        ),
        With<TextView>,
    >,
    font: Res<FontSettings>,
    mut atlas: ResMut<GlyphAtlas>,
    mut images: ResMut<Assets<Image>>,
) {
    for (tv_entity, state, viewport, layout, overlays, batch_entity_opt, render_layers) in
        text_views.iter_mut()
    {
        // W5 skip-on-unchanged: if neither the display layout nor the overlays
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
