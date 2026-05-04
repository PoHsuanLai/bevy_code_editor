//! Editor-specific instanced GPU text rendering
//!
//! This module is a thin wrapper around the generic `text_view::render` module.
//! It populates styled lines from syntax highlighting and delegates to the
//! generic renderer.

use super::SyntaxResource;
use crate::gpu_text::GlyphAtlas;
use crate::settings::*;
use crate::text_view::{TextViewOverlays, TextViewState, TextViewViewport};
use crate::types::*;
use bevy::prelude::*;

// Re-export types from text_view for backward compatibility
pub use crate::text_view::render::{GlyphBatchComponent, GlyphInstance, TextViewBatch};

/// Legacy alias for TextViewBatch — used by existing editor code
pub type GpuTextBatch = TextViewBatch;

/// Marker component to distinguish code editor batches from other TextViewBatch users.
#[derive(Component)]
pub struct CodeEditorBatch;

/// System to update instanced GPU text display for the editor.
///
/// Reads the TextViewState component directly from the editor entity,
/// populates styled lines from syntax highlighting, and delegates to
/// the generic `render_text_view()`.
pub(crate) fn update_gpu_text_instanced(
    mut commands: Commands,
    mut tv_query: Query<
        (&mut TextViewState, &TextViewViewport, Ref<TextViewOverlays>),
        With<CodeEditor>,
    >,
    (font, theme, syntax_settings, ui_settings, performance): (
        Res<FontSettings>,
        Res<ThemeSettings>,
        Res<SyntaxSettings>,
        Res<UiSettings>,
        Res<PerformanceSettings>,
    ),
    #[cfg(feature = "folding")] fold_state: Res<FoldState>,
    mut atlas: ResMut<GlyphAtlas>,
    mut images: ResMut<Assets<Image>>,
    batch_query: Query<(Entity, &TextViewBatch), With<CodeEditorBatch>>,
    mut syntax: ResMut<SyntaxResource>,
    time: Res<Time>,
    render_config: Option<Res<super::editor_ui_plugin::EditorRenderConfig>>,
) {
    // Try to get the entity's TextViewState; fall back to Resource viewport if entity not ready
    let Ok((mut tv_state, tv_viewport, tv_overlays)) = tv_query.single_mut() else {
        return;
    };
    let overlays_changed = tv_overlays.is_changed();

    // Check if viewport changed
    let viewport_changed = if let Some((_, batch)) = batch_query.iter().next() {
        batch.built_at_width != tv_viewport.width || batch.built_at_height != tv_viewport.height
    } else {
        true
    };

    // Use the entity's TextViewState for update checks
    if !tv_state.needs_update
        && !tv_state.needs_scroll_update
        && !viewport_changed
        && !overlays_changed
    {
        return;
    }

    #[cfg(not(feature = "folding"))]
    let fold_state = FoldState::default();

    let line_height = font.line_height;
    let buffer_lines = line_height * performance.viewport_buffer_lines as f32;
    let scroll_dist = tv_state.scroll_offset.abs();
    let start_pixels = scroll_dist - tv_viewport.text_area_top - buffer_lines;
    let first_visible = (start_pixels / line_height).floor().max(0.0) as usize;
    let visible_count =
        ((tv_viewport.height as f32 + buffer_lines * 2.0) / line_height).ceil() as usize;
    let last_visible = first_visible + visible_count;

    let total_lines = tv_state.line_count();

    // Populate styled lines from syntax highlighting for visible range
    let has_folding = !fold_state.regions.is_empty();
    let start_buffer_line = if has_folding {
        let mut display_row = 0;
        let mut buf_line = 0;
        while buf_line < total_lines && display_row < first_visible {
            if !fold_state.is_line_hidden(buf_line) {
                display_row += 1;
            }
            buf_line += 1;
        }
        buf_line
    } else {
        first_visible.min(total_lines)
    };

    // Ensure styled_lines has capacity
    tv_state.styled_lines.resize_with(total_lines, || None);

    // Populate syntax highlighting for visible lines
    let mut display_row = if has_folding {
        let mut dr = 0;
        let mut bl = 0;
        while bl < start_buffer_line {
            if !fold_state.is_line_hidden(bl) {
                dr += 1;
            }
            bl += 1;
        }
        dr
    } else {
        start_buffer_line
    };

    for buffer_line in start_buffer_line..total_lines {
        if fold_state.is_line_hidden(buffer_line) {
            continue;
        }
        if display_row > last_visible {
            break;
        }

        let rope_line = tv_state.rope.line(buffer_line);
        let line_string = rope_line.to_string();

        let segments_vec = syntax.highlight_range(
            &line_string,
            buffer_line,
            buffer_line + 1,
            tv_state.rope.line_to_byte(buffer_line),
            &syntax_settings.theme,
            theme.foreground,
        );

        if !segments_vec.is_empty() {
            tv_state.styled_lines[buffer_line] = Some(segments_vec.into_iter().next().unwrap());
        }

        display_row += 1;
    }

    // Calculate content start X
    let content_start_x = tv_viewport
        .text_area_left
        .max(tv_viewport.gutter_width + ui_settings.code_margin_left);

    // === Step 5b: render_layout is now the on-screen renderer ===
    // Build a DisplayLayout snapshot and render through the new path. The legacy
    // render_text_view stays alive only for the equivalence diagnostic in debug
    // builds; both go away in step 9.
    use crate::display_map::layout::build_display_layout;
    use crate::text_view::render::render_layout;
    let layout = build_display_layout(
        &tv_state,
        tv_viewport,
        &fold_state,
        &font,
        &performance,
        theme.foreground,
    );
    let instances = render_layout(
        &layout,
        Some(&tv_overlays),
        tv_viewport,
        &mut atlas,
        content_start_x,
        tv_state.horizontal_scroll_offset,
        font.size,
    );

    // The equivalence diagnostic against `render_text_view` was removed: the
    // new path uses a unified row-top coordinate convention, while the legacy
    // path used row-center (with each component layering its own correction).
    // The two paths are intentionally numerically different now, so comparing
    // them only produces noise. `render_text_view` is dead code pending its
    // deletion in step 9.

    // Update atlas texture
    atlas.update_texture(&mut images);

    // Extract render layer number from config
    let batch_render_layer: Option<u8> = render_config.as_ref().and_then(|config| {
        config.render_layers.as_ref().and_then(|layers| {
            (0u8..=31).find(|&i| {
                layers.intersects(&bevy_camera::visibility::RenderLayers::layer(i as usize))
            })
        })
    });

    // Update or create batch entity
    if instances.is_empty() {
        for (entity, _) in batch_query.iter() {
            commands.entity(entity).insert(GlyphBatchComponent {
                instances: Vec::new(),
                atlas_texture: atlas.texture.clone(),
                render_layer: batch_render_layer,
            });
            commands.entity(entity).insert(Visibility::Hidden);
        }
    } else {
        let existing_batches: Vec<Entity> = batch_query.iter().map(|(e, _)| e).collect();

        if let Some(&first_entity) = existing_batches.first() {
            let mut cmds = commands.entity(first_entity);
            cmds.insert(GlyphBatchComponent {
                instances,
                atlas_texture: atlas.texture.clone(),
                render_layer: batch_render_layer,
            });
            cmds.insert(Visibility::Visible);
            cmds.insert(TextViewBatch {
                built_at_scroll: tv_state.scroll_offset,
                built_at_horizontal_scroll: tv_state.horizontal_scroll_offset,
                first_line: first_visible,
                last_line: last_visible,
                built_at_width: tv_viewport.width,
                built_at_height: tv_viewport.height,
            });
            // Ensure render layers are applied on existing batch too
            if let Some(ref config) = render_config {
                if let Some(ref layers) = config.render_layers {
                    cmds.insert(layers.clone());
                }
            }

            for &entity in &existing_batches[1..] {
                commands.entity(entity).despawn();
            }
        } else {
            let mut entity_cmds = commands.spawn((
                GlyphBatchComponent {
                    instances,
                    atlas_texture: atlas.texture.clone(),
                    render_layer: batch_render_layer,
                },
                Transform::default(),
                GlobalTransform::default(),
                TextViewBatch {
                    built_at_scroll: tv_state.scroll_offset,
                    built_at_horizontal_scroll: tv_state.horizontal_scroll_offset,
                    first_line: first_visible,
                    last_line: last_visible,
                    built_at_width: tv_viewport.width,
                    built_at_height: tv_viewport.height,
                },
                CodeEditorBatch,
                Name::new("GpuTextBatch"),
                Visibility::Visible,
                InheritedVisibility::default(),
                ViewVisibility::default(),
            ));
            // Apply render layers from EditorRenderConfig so the batch is visible
            // only to the code editor camera (not the main camera or other text view cameras)
            if let Some(ref config) = render_config {
                if let Some(ref layers) = config.render_layers {
                    entity_cmds.insert(layers.clone());
                }
            }
        }
    }

    tv_state.needs_update = false;
    tv_state.needs_scroll_update = false;
    tv_state.last_render_time = time.elapsed_secs_f64() * 1000.0;
}
