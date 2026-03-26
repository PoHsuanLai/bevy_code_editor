//! Editor-specific instanced GPU text rendering
//!
//! This module is a thin wrapper around the generic `text_view::render` module.
//! It populates styled lines from syntax highlighting and delegates to the
//! generic renderer.

use super::SyntaxResource;
use crate::gpu_text::GlyphAtlas;
use crate::settings::*;
use crate::text_view::{TextViewState, TextViewViewport};
use crate::types::*;
use bevy::prelude::*;
use std::sync::Arc;

// Re-export types from text_view for backward compatibility
pub use crate::text_view::render::{GlyphBatchComponent, GlyphInstance, TextViewBatch};

/// Legacy alias for TextViewBatch — used by existing editor code
pub type GpuTextBatch = TextViewBatch;

/// System to update instanced GPU text display for the editor.
///
/// Reads the TextViewState component directly from the editor entity,
/// populates styled lines from syntax highlighting, and delegates to
/// the generic `render_text_view()`.
pub(crate) fn update_gpu_text_instanced(
    mut commands: Commands,
    mut tv_query: Query<(&mut TextViewState, &TextViewViewport), With<CodeEditor>>,
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
    batch_query: Query<(Entity, &TextViewBatch)>,
    mut syntax: ResMut<SyntaxResource>,
    time: Res<Time>,
) {
    // Try to get the entity's TextViewState; fall back to Resource viewport if entity not ready
    let Ok((mut tv_state, tv_viewport)) = tv_query.single_mut() else {
        return;
    };

    // Check if viewport changed
    let viewport_changed = if let Some((_, batch)) = batch_query.iter().next() {
        batch.built_at_width != tv_viewport.width || batch.built_at_height != tv_viewport.height
    } else {
        true
    };

    // Use the entity's TextViewState for update checks
    if !tv_state.needs_update && !tv_state.needs_scroll_update && !viewport_changed {
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
    tv_state
        .styled_lines
        .resize_with(total_lines, || None);

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

    // Delegate to generic renderer
    let instances = crate::text_view::render::render_text_view(
        &tv_state,
        tv_viewport,
        Some(&fold_state),
        &font,
        &performance,
        theme.foreground,
        &mut atlas,
        content_start_x,
    );

    // Update atlas texture
    atlas.update_texture(&mut images);

    // Update or create batch entity
    if arc_instances.is_empty() {
        for (entity, _, mut batch_comp) in batch_query.iter_mut() {
            // Clear instances immediately — no deferred commands, takes effect this frame
            batch_comp.instances = arc_instances.clone();
            commands.entity(entity).insert(Visibility::Hidden);
        }
    } else {
        let existing_batches: Vec<Entity> = batch_query.iter().map(|(e, _, _)| e).collect();

        if let Some(&first_entity) = existing_batches.first() {
            commands.entity(first_entity).insert(GlyphBatchComponent {
                instances: arc_instances,
                atlas_texture: atlas.texture.clone(),
            });
            commands.entity(first_entity).insert(Visibility::Visible);
            commands.entity(first_entity).insert(TextViewBatch {
                built_at_scroll: tv_state.scroll_offset,
                built_at_horizontal_scroll: tv_state.horizontal_scroll_offset,
                first_line: first_visible,
                last_line: last_visible,
                built_at_width: tv_viewport.width,
                built_at_height: tv_viewport.height,
            });

            for &entity in &existing_batches[1..] {
                commands.entity(entity).despawn();
            }
        } else {
            commands.spawn((
                GlyphBatchComponent {
                    instances: arc_instances,
                    atlas_texture: atlas.texture.clone(),
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
                Name::new("GpuTextBatch"),
                Visibility::Visible,
                InheritedVisibility::default(),
                ViewVisibility::default(),
            ));
        }
    }

    tv_state.needs_update = false;
    tv_state.needs_scroll_update = false;
    tv_state.last_render_time = time.elapsed_secs_f64() * 1000.0;
}
