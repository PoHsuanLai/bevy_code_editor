//! TextViewPlugin — standalone Bevy plugin for rendering text views.
//!
//! This plugin provides everything needed to render styled text in a scrollable viewport
//! without any editor-specific concerns. It can be used independently for chat panels,
//! log viewers, or any other text display.

use bevy::prelude::*;

use super::render::{render_text_view, GlyphBatchComponent, TextViewBatch};
use super::state::TextViewState;
use super::viewport::TextViewViewport;
use crate::gpu_text::GlyphAtlas;
use crate::settings::{FontSettings, PerformanceSettings, ThemeSettings};

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
        // Add GPU rendering infrastructure
        app.add_plugins(crate::gpu_text::GpuTextPlugin);
        app.add_plugins(crate::gpu_text::InstancedTextRenderPlugin);

        // Text view systems
        app.add_systems(
            Update,
            (
                debounce_text_view_updates,
                animate_text_view_scroll,
                update_text_views
                    .run_if(crate::gpu_text::atlas_ready)
                    .in_set(TextViewRenderSet),
            )
                .chain(),
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

/// Debounce interval for text view updates (~60fps)
const DEBOUNCE_INTERVAL_MS: f64 = 16.0;

/// Debouncing system for text views — promotes pending_update to needs_update
fn debounce_text_view_updates(
    mut query: Query<&mut TextViewState, With<TextView>>,
    time: Res<Time>,
) {
    for mut state in query.iter_mut() {
        if !state.pending_update {
            continue;
        }

        let current_time = time.elapsed_secs_f64() * 1000.0;
        let elapsed = current_time - state.last_render_time;

        if elapsed >= DEBOUNCE_INTERVAL_MS {
            state.needs_update = true;
            state.pending_update = false;
            state.last_render_time = current_time;
        }
    }
}

/// Smooth scroll animation for text views
fn animate_text_view_scroll(mut query: Query<&mut TextViewState, With<TextView>>, time: Res<Time>) {
    let dt = time.delta_secs();
    let lerp_speed = 12.0; // Exponential decay factor

    for mut state in query.iter_mut() {
        // Vertical scroll
        let diff_v = state.target_scroll_offset - state.scroll_offset;
        if diff_v.abs() > 0.5 {
            state.scroll_offset += diff_v * (1.0 - (-lerp_speed * dt).exp());
            state.needs_scroll_update = true;
        } else if diff_v.abs() > 0.001 {
            state.scroll_offset = state.target_scroll_offset;
            state.needs_scroll_update = true;
        }

        // Horizontal scroll
        let diff_h = state.target_horizontal_scroll_offset - state.horizontal_scroll_offset;
        if diff_h.abs() > 0.5 {
            state.horizontal_scroll_offset += diff_h * (1.0 - (-lerp_speed * dt).exp());
            state.needs_scroll_update = true;
        } else if diff_h.abs() > 0.001 {
            state.horizontal_scroll_offset = state.target_horizontal_scroll_offset;
            state.needs_scroll_update = true;
        }
    }
}

/// Main rendering system for all text views.
///
/// Queries all entities with `TextView` + `TextViewState` + `TextViewViewport`
/// and renders each one using `render_text_view()`, managing per-entity batch children.
fn update_text_views(
    mut commands: Commands,
    mut text_views: Query<
        (
            Entity,
            &mut TextViewState,
            &TextViewViewport,
            Option<&TextViewBatchEntity>,
        ),
        With<TextView>,
    >,
    batch_query: Query<(Entity, &TextViewBatch)>,
    font: Res<FontSettings>,
    theme: Res<ThemeSettings>,
    performance: Res<PerformanceSettings>,
    mut atlas: ResMut<GlyphAtlas>,
    mut images: ResMut<Assets<Image>>,
    time: Res<Time>,
) {
    for (tv_entity, mut state, viewport, batch_entity_opt) in text_views.iter_mut() {
        // Check if viewport changed
        let viewport_changed = if let Some(batch_e) = batch_entity_opt {
            if let Ok((_, batch)) = batch_query.get(batch_e.0) {
                batch.built_at_width != viewport.width || batch.built_at_height != viewport.height
            } else {
                true
            }
        } else {
            true
        };

        if !state.needs_update && !state.needs_scroll_update && !viewport_changed {
            continue;
        }

        // Calculate content start X (for text views without gutter, starts at text_area_left)
        let content_start_x = if viewport.gutter_width > 0.0 {
            viewport.text_area_left.max(viewport.gutter_width)
        } else {
            viewport.text_area_left
        };

        // Render
        let instances = render_text_view(
            &state,
            viewport,
            None, // No folding for generic text views
            &font,
            &performance,
            theme.foreground,
            &mut atlas,
            content_start_x,
        );

        // Update atlas texture
        atlas.update_texture(&mut images);

        // Calculate visible range for batch metadata
        let line_height = font.line_height;
        let buffer_lines = line_height * performance.viewport_buffer_lines as f32;
        let scroll_dist = state.scroll_offset.abs();
        let start_pixels = scroll_dist - viewport.text_area_top - buffer_lines;
        let first_visible = (start_pixels / line_height).floor().max(0.0) as usize;
        let visible_count =
            ((viewport.height as f32 + buffer_lines * 2.0) / line_height).ceil() as usize;
        let last_visible = first_visible + visible_count;

        let batch_data = TextViewBatch {
            built_at_scroll: state.scroll_offset,
            built_at_horizontal_scroll: state.horizontal_scroll_offset,
            first_line: first_visible,
            last_line: last_visible,
            built_at_width: viewport.width,
            built_at_height: viewport.height,
        };

        // Update or create batch entity
        if instances.is_empty() {
            if let Some(batch_e) = batch_entity_opt {
                commands.entity(batch_e.0).insert(Visibility::Hidden);
            }
        } else {
            let batch_comp = GlyphBatchComponent {
                instances,
                atlas_texture: atlas.texture.clone(),
            };

            if let Some(batch_e) = batch_entity_opt {
                // Update existing batch entity
                commands
                    .entity(batch_e.0)
                    .insert(batch_comp)
                    .insert(Visibility::Visible)
                    .insert(batch_data);
            } else {
                // Spawn new batch entity as child
                let batch_entity = commands
                    .spawn((
                        batch_comp,
                        Transform::default(),
                        GlobalTransform::default(),
                        batch_data,
                        Name::new("TextViewBatch"),
                        Visibility::Visible,
                        InheritedVisibility::default(),
                        ViewVisibility::default(),
                    ))
                    .id();

                commands
                    .entity(tv_entity)
                    .insert(TextViewBatchEntity(batch_entity));
            }
        }

        state.needs_update = false;
        state.needs_scroll_update = false;
        state.last_render_time = time.elapsed_secs_f64() * 1000.0;
    }
}
