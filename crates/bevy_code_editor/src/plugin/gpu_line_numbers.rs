//! GPU-accelerated line numbers rendering
//!
//! Uses the same instanced rendering pipeline as the main code text for visual consistency.

use bevy_text_engine::gpu::GlyphAtlas;
use bevy_text_engine::FontConfig;
use crate::settings::*;
use crate::text_view::render::{GlyphBatchComponent, GlyphInstance};
use crate::text_view::{ScrollState, TextBuffer, TextViewViewport};
use crate::types::*;
use bevy::prelude::*;

/// Marker component for the GPU line numbers batch entity
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct GpuLineNumbersBatch {
    /// Editor entity this batch belongs to
    pub editor: Entity,
    /// Content version when this batch was built
    pub built_at_version: u64,
    /// Scroll offset when this batch was built
    pub built_at_scroll: f32,
    /// Viewport dimensions when built
    pub built_at_width: u32,
    pub built_at_height: u32,
}

/// GPU-accelerated line numbers rendering system
pub(crate) fn update_gpu_line_numbers(
    mut commands: Commands,
    editor_query: Query<
        (
            Entity,
            &SelectionState,
            &TextBuffer,
            &ScrollState,
            &TextViewViewport,
            Ref<FoldState>,
            &FontConfig,
            &ThemeConfig,
        ),
        With<CodeEditor>,
    >,
    ui: Res<UiSettings>,
    performance: Res<PerformanceSettings>,
    mut atlas: ResMut<GlyphAtlas>,
    mut images: ResMut<Assets<Image>>,
    fonts: Res<Assets<bevy::text::Font>>,
    batch_query: Query<(Entity, &GpuLineNumbersBatch)>,
) {
    // Hide if line numbers are disabled
    if !ui.show_line_numbers {
        for (entity, _) in batch_query.iter() {
            commands.entity(entity).insert(Visibility::Hidden);
        }
        return;
    }

    for (editor_entity, sel, buffer, scroll, viewport, fold_state, font, theme) in editor_query.iter() {
    // Check if we need to update
    let fold_changed = fold_state.is_changed();

    let existing_batch_for_editor = batch_query
        .iter()
        .find(|(_, b)| b.editor == editor_entity);

    if !fold_changed {
        // Check if existing batch is still valid
        if let Some((entity, batch)) = existing_batch_for_editor {
            let scroll_changed = (batch.built_at_scroll - scroll.scroll_offset).abs() > 0.01;
            let viewport_changed =
                batch.built_at_width != viewport.width || batch.built_at_height != viewport.height;

            if !scroll_changed && !viewport_changed && batch.built_at_version == buffer.content_version
            {
                commands.entity(entity).insert(Visibility::Visible);
                continue;
            }
        }
    }

    let line_height = font.line_height;
    let font_size = font.font_size;
    let _viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;

    // Collect cursor lines for highlighting active line numbers
    let cursor_lines: std::collections::HashSet<usize> = sel
        .selections
        .iter()
        .map(|s| {
            let pos = s.head_offset().min(buffer.rope.len_chars());
            buffer.rope.char_to_line(pos)
        })
        .collect();

    // Calculate visible line range
    let buffer_lines = performance.viewport_buffer_lines as f32;
    let viewport_top = -scroll.scroll_offset - line_height * buffer_lines;
    let viewport_bottom = viewport_top + viewport_height + line_height * buffer_lines * 2.0;

    let first_visible_display_row = ((viewport_top - viewport.text_area_top) / line_height)
        .floor()
        .max(0.0) as usize;
    let last_visible_display_row =
        ((viewport_bottom - viewport.text_area_top) / line_height).ceil() as usize;

    let total_buffer_lines = buffer.line_count();
    let has_folding = fold_state.regions.iter().any(|r| r.is_folded);

    // Calculate starting buffer line and display row
    let (start_buffer_line, mut current_display_row) = if has_folding {
        let actual = fold_state
            .display_to_actual_line(first_visible_display_row)
            .min(total_buffer_lines);
        (actual, first_visible_display_row)
    } else {
        let start = first_visible_display_row.min(total_buffer_lines);
        (start, start)
    };

    // Snapshot the engine's row anchor once. The line-number batch
    // emits glyphs directly (it doesn't go through `render_layout`),
    // but its baseline must match what the engine paints for the main
    // text on the same row — `glyph_baseline_screen_y` is the engine's
    // own formula exposed as a public helper.
    let metrics = bevy_text_engine::row_metrics(viewport, scroll, font);

    // Calculate gutter center X position (camera-relative, not world coords)
    // Camera is at viewport center, so gutter is at -viewport_width/2 + gutter_width/2
    let gutter_center_x = viewport.world_left() + viewport.gutter_width / 2.0;

    // Pre-allocate instances
    let estimated_capacity = (last_visible_display_row - first_visible_display_row + 2) * 4;
    let mut instances: Vec<GlyphInstance> = Vec::with_capacity(estimated_capacity);

    // Pre-collect folded regions that intersect the visible buffer window,
    // sorted by start_line. We advance a cursor through them as we walk
    // visible lines, jumping over hidden tails in O(1) per fold rather than
    // probing `is_line_hidden` (O(n_regions)) for every visible line.
    let mut folded_iter = fold_state
        .regions
        .iter()
        .filter(|r| r.is_folded && r.end_line >= start_buffer_line)
        .peekable();

    // Iterate over visible buffer lines
    let mut buffer_line = start_buffer_line;
    while buffer_line < total_buffer_lines {
        // Skip past any fold whose hidden tail covers this line.
        while let Some(fold) = folded_iter.peek() {
            if fold.end_line < buffer_line {
                folded_iter.next();
                continue;
            }
            // Fold's placeholder row is `start_line` (visible); rows
            // `start_line+1..=end_line` are hidden.
            if buffer_line > fold.start_line && buffer_line <= fold.end_line {
                buffer_line = fold.end_line + 1;
                continue;
            }
            break;
        }
        if buffer_line >= total_buffer_lines {
            break;
        }

        if current_display_row > last_visible_display_row {
            break;
        }

        // Glyph baseline matches the main-text row's baseline by
        // construction; if the engine ever changes its baseline math,
        // `glyph_baseline_screen_y` updates and this code follows.
        let base_y = metrics.glyph_baseline_screen_y(current_display_row as u32);

        // Line number text (1-indexed)
        let line_number_text = (buffer_line + 1).to_string();

        // Use active color for cursor lines
        let line_color = if cursor_lines.contains(&buffer_line) {
            theme.line_numbers_active
        } else {
            theme.line_numbers
        };
        let color_linear = line_color.to_linear();
        let color_arr = [
            color_linear.red,
            color_linear.green,
            color_linear.blue,
            color_linear.alpha,
        ];

        // Shape the line number text via cosmic-text. `shape.width` gives an
        // exact pixel width for right-alignment, and `shape.glyphs` carry the
        // per-glyph pen-x and atlas cache_key the renderer needs.
        let font_id = atlas.ensure_font(&font.font, &fonts);
        let shape = atlas.shape_line(&line_number_text, font_size, font_id);

        // Right-align: start X so that text ends near the right edge of gutter (with padding)
        let right_padding = 8.0;
        let start_x =
            gutter_center_x + viewport.gutter_width / 2.0 - right_padding - shape.width;

        // Emit each shaped glyph
        for g in &shape.glyphs {
            let Some((info, _)) = atlas.get_or_rasterize_glyph(g.cache_key) else {
                continue;
            };

            // Skip degenerate glyphs (zero-width or zero-height). Fonts
            // can produce these for whitespace, joiners, or trailing pen
            // positions; emitting a quad with one zero dimension still
            // draws a faint 1-pixel line where bilinear sampling bleeds.
            if info.size.x <= 0.0 || info.size.y <= 0.0 {
                continue;
            }

            // Calculate screen position (same logic as main text)
            let screen_y = base_y - info.offset.y;

            // Convert to camera-relative world coordinates. Mirrors the
            // engine's `glyph_quad`: `world_y = world_top - screen_y -
            // glyph_height`. Camera is at viewport center, entities
            // positioned relative to camera.
            let world_x = start_x + g.x + info.offset.x;
            let world_y = metrics.world_top - screen_y - info.size.y;

            let instance = GlyphInstance {
                position: Vec2::new(world_x, world_y),
                uv_min: info.uv_min,
                uv_max: info.uv_max,
                size: info.size,
                color: color_arr,
                z_index: 0.0, // Line numbers at same level as main text
                corner_radii: [0.0; 4],
                skew: 0.0,
                _padding: [0.0; 2],
            };

            instances.push(instance);
        }

        current_display_row += 1;
        buffer_line += 1;
    }

    // Update atlas texture
    atlas.update_texture(&mut images);

    if instances.is_empty() {
        if let Some((entity, _)) = batch_query
            .iter()
            .find(|(_, b)| b.editor == editor_entity)
        {
            commands.entity(entity).insert(Visibility::Hidden);
        }
        continue;
    }

    // Update or create batch entity for this editor
    if let Some((entity, _)) = batch_query
        .iter()
        .find(|(_, b)| b.editor == editor_entity)
    {
        commands
            .entity(entity)
            .insert(GlyphBatchComponent {
                instances,
                atlas_texture: atlas.texture.clone(),
                render_layer: None,
            })
            .insert(GpuLineNumbersBatch {
                editor: editor_entity,
                built_at_version: buffer.content_version,
                built_at_scroll: scroll.scroll_offset,
                built_at_width: viewport.width,
                built_at_height: viewport.height,
            })
            .insert(Visibility::Visible);
    } else {
        // Parent under the editor entity so the editor's `Transform`
        // cascades. The unit-quad `Mesh2d` is required by the
        // `Mesh2dPipeline` view extraction so this batch gets a per-
        // entity `world_from_local` mesh bind group.
        commands.spawn((
            GlyphBatchComponent {
                instances,
                atlas_texture: atlas.texture.clone(),
                render_layer: None,
            },
            Transform::default(),
            GlobalTransform::default(),
            GpuLineNumbersBatch {
                editor: editor_entity,
                built_at_version: buffer.content_version,
                built_at_scroll: scroll.scroll_offset,
                built_at_width: viewport.width,
                built_at_height: viewport.height,
            },
            Name::new("GpuLineNumbersBatch"),
            Visibility::Visible,
            InheritedVisibility::default(),
            ViewVisibility::default(),
        ));
    }
    }
}
