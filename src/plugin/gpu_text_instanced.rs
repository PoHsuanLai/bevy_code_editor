//! Editor-specific instanced GPU text rendering
//!
//! This module is a thin wrapper around the generic `text_view::render` module.
//! It populates styled lines from syntax highlighting and delegates to the
//! generic renderer.

use super::SyntaxResource;
use crate::gpu_text::GlyphAtlas;
use crate::settings::*;
use crate::types::*;
use bevy::prelude::*;
use std::sync::Arc;

// Re-export types from text_view for backward compatibility
pub use crate::text_view::render::{GlyphBatchComponent, GlyphInstance, TextViewBatch};

/// Legacy alias for TextViewBatch — used by existing editor code
pub type GpuTextBatch = TextViewBatch;

/// System to update instanced GPU text display for the editor.
///
/// This is the editor-specific wrapper that:
/// 1. Populates styled lines from syntax highlighting
/// 2. Delegates to `text_view::render::render_text_view()`
/// 3. Manages the batch entity
pub(crate) fn update_gpu_text_instanced(
    mut commands: Commands,
    mut state: ResMut<CodeEditorState>,
    (font, theme, syntax_settings, ui_settings, performance): (
        Res<FontSettings>,
        Res<ThemeSettings>,
        Res<SyntaxSettings>,
        Res<UiSettings>,
        Res<PerformanceSettings>,
    ),
    viewport: Res<ViewportDimensions>,
    #[cfg(feature = "folding")] fold_state: Res<FoldState>,
    mut atlas: ResMut<GlyphAtlas>,
    mut images: ResMut<Assets<Image>>,
    batch_query: Query<(Entity, &TextViewBatch)>,
    mut syntax: ResMut<SyntaxResource>,
    time: Res<Time>,
) {
    // Check if viewport changed
    let viewport_changed = if let Some((_, batch, _)) = batch_query.iter().next() {
        batch.built_at_width != viewport.width || batch.built_at_height != viewport.height
    } else {
        true
    };

    if !state.needs_update && !state.needs_scroll_update && !viewport_changed {
        return;
    }

    #[cfg(not(feature = "folding"))]
    let fold_state = FoldState::default();

    // Build a temporary TextViewState-like view for the generic renderer.
    // We construct a TextViewViewport from ViewportDimensions.
    let tv_viewport = crate::text_view::TextViewViewport {
        width: viewport.width,
        height: viewport.height,
        offset_x: viewport.offset_x,
        offset_y: viewport.offset_y,
        text_area_left: viewport.text_area_left,
        text_area_top: viewport.text_area_top,
        gutter_width: viewport.gutter_width,
        separator_x: viewport.separator_x,
    };

    // Populate styled lines from syntax highlighting for visible range
    let line_height = font.line_height;
    let buffer_lines = line_height * performance.viewport_buffer_lines as f32;
    let scroll_dist = state.scroll_offset.abs();
    let start_pixels = scroll_dist - viewport.text_area_top - buffer_lines;
    let first_visible = (start_pixels / line_height).floor().max(0.0) as usize;
    let visible_count =
        ((viewport.height as f32 + buffer_lines * 2.0) / line_height).ceil() as usize;
    let last_visible = first_visible + visible_count;

    let total_lines = state.line_count();

    // Build a temporary TextViewState for the generic renderer
    let mut tv_state = crate::text_view::TextViewState {
        rope: state.rope.clone(),
        scroll_offset: state.scroll_offset,
        target_scroll_offset: state.target_scroll_offset,
        horizontal_scroll_offset: state.horizontal_scroll_offset,
        target_horizontal_scroll_offset: state.target_horizontal_scroll_offset,
        needs_update: state.needs_update,
        needs_scroll_update: state.needs_scroll_update,
        pending_update: state.pending_update,
        last_render_time: state.last_render_time,
        content_version: state.content_version,
        dirty_lines: state.dirty_lines.clone(),
        previous_line_count: state.previous_line_count,
        max_content_width: state.max_content_width,
        max_content_width_version: state.max_content_width_version,
        max_width_line: state.max_width_line,
        line_width_tracker: state.line_width_tracker.clone(),
        styled_lines: Vec::new(),
        styled_lines_version: 0,
    };

    // Populate styled lines from syntax for visible range
    // (accounting for folding to find actual buffer lines)
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

        let rope_line = state.rope.line(buffer_line);
        let line_string = rope_line.to_string();

        let segments_vec = syntax.highlight_range(
            &line_string,
            buffer_line,
            buffer_line + 1,
            state.rope.line_to_byte(buffer_line),
            &syntax_settings.theme,
            theme.foreground,
        );

        if !segments_vec.is_empty() {
            tv_state.styled_lines[buffer_line] = Some(segments_vec.into_iter().next().unwrap());
        }

        display_row += 1;
    }

    // Calculate content start X
    let content_start_x = viewport
        .text_area_left
        .max(viewport.gutter_width + ui_settings.code_margin_left);

    // Delegate to generic renderer
    let instances = crate::text_view::render::render_text_view(
        &tv_state,
        &tv_viewport,
        Some(&fold_state),
        &font,
        &performance,
        theme.foreground,
        &mut atlas,
        content_start_x,
    );

    // Update atlas texture
    atlas.update_texture(&mut images);

    // Calculate visible range for batch metadata
    let first_visible_display_row = first_visible;
    let last_visible_display_row = last_visible;

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
                built_at_scroll: state.scroll_offset,
                built_at_horizontal_scroll: state.horizontal_scroll_offset,
                first_line: first_visible_display_row,
                last_line: last_visible_display_row,
                built_at_width: viewport.width,
                built_at_height: viewport.height,
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
                    built_at_scroll: state.scroll_offset,
                    built_at_horizontal_scroll: state.horizontal_scroll_offset,
                    first_line: first_visible_display_row,
                    last_line: last_visible_display_row,
                    built_at_width: viewport.width,
                    built_at_height: viewport.height,
                },
                Name::new("GpuTextBatch"),
                Visibility::Visible,
                InheritedVisibility::default(),
                ViewVisibility::default(),
            ));
        }
    }

    // If budget was exceeded, request another frame to finish remaining lines
    if budget_exceeded {
        state.needs_update = true;
    } else {
        state.needs_update = false;
    }
    state.needs_scroll_update = false;
    state.last_render_time = time.elapsed_secs_f64() * 1000.0;
}
