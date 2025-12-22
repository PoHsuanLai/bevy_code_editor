//! Cursor rendering and animation

use bevy::prelude::*;
use crate::settings::EditorSettings;
use crate::types::*;
use super::to_bevy_coords_left_aligned;

pub(crate) fn update_cursor(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    fold_state: Res<FoldState>,
    mut cursor_query: Query<(Entity, &EditorCursor, &mut Transform, &mut Visibility)>,
) {
    if !state.is_changed() {
        return;
    }

    let char_width = settings.font.char_width;
    let line_height = settings.font.line_height;
    let cursor_height = line_height * settings.cursor.height_multiplier;
    let cursor_count = state.cursors.len();

    // Check if we're using soft line wrapping
    let use_wrapping = settings.wrapping.enabled && state.display_map.wrap_width > 0;

    // Collect existing cursor entities by their index
    let mut cursor_entities: std::collections::HashMap<usize, Entity> = std::collections::HashMap::new();
    for (entity, cursor, _, _) in cursor_query.iter() {
        cursor_entities.insert(cursor.cursor_index, entity);
    }

    // Update or create cursor entities for each cursor
    for (idx, cursor) in state.cursors.iter().enumerate() {
        let cursor_pos = cursor.position.min(state.rope.len_chars());
        let line_index = state.rope.char_to_line(cursor_pos);
        let line_start = state.rope.line_to_char(line_index);
        let col_index = cursor_pos - line_start;

        // Calculate display row and column based on wrapping and folding
        let (display_row, display_col) = if use_wrapping {
            state.display_map.buffer_to_display(line_index, col_index)
        } else {
            // Account for folded lines
            let display_row = fold_state.actual_to_display_line(line_index);
            (display_row, col_index)
        };

        // For wrapped continuation rows, add indent offset
        let extra_indent = if use_wrapping && settings.wrapping.indent_wrapped_lines {
            if state.display_map.is_continuation(display_row) {
                settings.indentation.indent_size as f32 * char_width
            } else {
                0.0
            }
        } else {
            0.0
        };

        let x_offset = settings.ui.layout.code_margin_left + extra_indent + (display_col as f32 * char_width);
        let y_offset = settings.ui.layout.margin_top + state.scroll_offset + (display_row as f32 * line_height);

        // No horizontal scroll in wrapped mode
        let h_scroll = if use_wrapping { 0.0 } else { state.horizontal_scroll_offset };

        let translation = to_bevy_coords_left_aligned(
            x_offset,
            y_offset,
            viewport.width as f32,
            viewport.height as f32,
            viewport.offset_x,
            h_scroll,
        );

        if let Some(&entity) = cursor_entities.get(&idx) {
            // Update existing cursor entity
            if let Ok((_, _, mut transform, mut visibility)) = cursor_query.get_mut(entity) {
                transform.translation = Vec3::new(translation.x, translation.y, 1.0);
                *visibility = Visibility::Visible;
            }
            cursor_entities.remove(&idx);
        } else {
            // Spawn new cursor entity
            commands.spawn((
                Sprite {
                    color: settings.theme.cursor,
                    custom_size: Some(Vec2::new(settings.cursor.width, cursor_height)),
                    ..default()
                },
                Transform::from_translation(Vec3::new(translation.x, translation.y, 1.0)),
                Visibility::Visible,
                EditorCursor { cursor_index: idx },
                Name::new(format!("EditorCursor_{}", idx)),
            ));
        }
    }

    // Hide or despawn excess cursor entities
    for (idx, entity) in cursor_entities {
        if idx < cursor_count {
            // This shouldn't happen, but hide just in case
            if let Ok((_, _, _, mut visibility)) = cursor_query.get_mut(entity) {
                *visibility = Visibility::Hidden;
            }
        } else {
            // Despawn cursor entities that are no longer needed
            commands.entity(entity).despawn();
        }
    }
}

/// Animate cursor blinking for all cursors
pub(crate) fn animate_cursor(
    time: Res<Time>,
    settings: Res<EditorSettings>,
    mut cursor_query: Query<&mut Visibility, With<EditorCursor>>,
) {
    if settings.cursor.blink_rate == 0.0 {
        for mut visibility in cursor_query.iter_mut() {
            *visibility = Visibility::Visible;
        }
        return;
    }

    let blink_phase = (time.elapsed_secs() * settings.cursor.blink_rate) % 1.0;
    let new_visibility = if blink_phase < 0.5 {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };

    for mut visibility in cursor_query.iter_mut() {
        *visibility = new_visibility;
    }
}
pub(crate) fn update_cursor_line_highlight(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    fold_state: Res<FoldState>,
    mut border_query: Query<(Entity, &CursorLineBorder, &mut Transform, &mut Sprite, &mut Visibility)>,
    mut word_query: Query<(Entity, &CursorWordHighlight, &mut Transform, &mut Sprite, &mut Visibility), Without<CursorLineBorder>>,
) {
    let cursor_line_settings = &settings.cursor_line;

    // Skip if cursor line highlighting is disabled entirely
    if !cursor_line_settings.enabled {
        // Hide all existing borders and word highlights
        for (_, _, _, _, mut visibility) in border_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        for (_, _, _, _, mut visibility) in word_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    // Get base highlight color from theme
    let base_highlight_color = match settings.theme.line_highlight {
        Some(color) => color,
        None => {
            // Hide all existing borders and word highlights
            for (_, _, _, _, mut visibility) in border_query.iter_mut() {
                *visibility = Visibility::Hidden;
            }
            for (_, _, _, _, mut visibility) in word_query.iter_mut() {
                *visibility = Visibility::Hidden;
            }
            return;
        }
    };

    if !state.is_changed() {
        return;
    }

    let line_height = settings.font.line_height;
    let char_width = settings.font.char_width;
    let use_wrapping = settings.wrapping.enabled && state.display_map.wrap_width > 0;

    // Border settings from configuration
    let border_thickness = cursor_line_settings.border_thickness;
    let border_color = cursor_line_settings.border_color.unwrap_or_else(|| {
        // Use base highlight color with configurable alpha multiplier
        Color::srgba(
            base_highlight_color.to_srgba().red,
            base_highlight_color.to_srgba().green,
            base_highlight_color.to_srgba().blue,
            (base_highlight_color.to_srgba().alpha * cursor_line_settings.border_alpha_multiplier).min(1.0),
        )
    });

    // Word highlight color from configuration
    let word_highlight_color = cursor_line_settings.word_highlight_color.unwrap_or(base_highlight_color);

    // Collect existing entities
    let mut border_entities: std::collections::HashMap<(usize, bool), Entity> = std::collections::HashMap::new();
    for (entity, border, _, _, _) in border_query.iter() {
        border_entities.insert((border.cursor_index, border.is_top), entity);
    }

    let mut word_entities: std::collections::HashMap<usize, Entity> = std::collections::HashMap::new();
    for (entity, word_hl, _, _, _) in word_query.iter() {
        word_entities.insert(word_hl.cursor_index, entity);
    }

    // Calculate border width (code area only, not the gutter)
    let code_area_start = settings.ui.layout.code_margin_left;
    let border_width = viewport.width as f32 - code_area_start;
    let border_center_x = -(viewport.width as f32) / 2.0 + code_area_start + border_width / 2.0 + viewport.offset_x;

    // Process each cursor
    for (idx, cursor) in state.cursors.iter().enumerate() {
        let cursor_pos = cursor.position.min(state.rope.len_chars());
        let line_index = state.rope.char_to_line(cursor_pos);

        // Skip if line is hidden due to folding
        if fold_state.is_line_hidden(line_index) {
            continue;
        }

        // Calculate display row
        let display_row = if use_wrapping {
            state.display_map.buffer_to_display(line_index, 0).0
        } else {
            let mut visible_row = line_index;
            for i in 0..line_index {
                if fold_state.is_line_hidden(i) {
                    visible_row = visible_row.saturating_sub(1);
                }
            }
            visible_row
        };

        let y_from_top = settings.ui.layout.margin_top + state.scroll_offset + (display_row as f32 * line_height);

        // === TOP BORDER ===
        if cursor_line_settings.show_border {
            let top_y = (viewport.height as f32) / 2.0 - y_from_top + line_height / 2.0 - border_thickness / 2.0;
            let top_translation = Vec3::new(border_center_x, top_y, -0.4);

            if let Some(&entity) = border_entities.get(&(idx, true)) {
                if let Ok((_, _, mut transform, mut sprite, mut visibility)) = border_query.get_mut(entity) {
                    transform.translation = top_translation;
                    sprite.custom_size = Some(Vec2::new(border_width, border_thickness));
                    sprite.color = border_color;
                    *visibility = Visibility::Visible;
                }
                border_entities.remove(&(idx, true));
            } else {
                commands.spawn((
                    Sprite {
                        color: border_color,
                        custom_size: Some(Vec2::new(border_width, border_thickness)),
                        ..default()
                    },
                    Transform::from_translation(top_translation),
                    Visibility::Visible,
                    CursorLineBorder { cursor_index: idx, is_top: true },
                    Name::new(format!("CursorLineBorder_top_{}", idx)),
                ));
            }

            // === BOTTOM BORDER ===
            let bottom_y = (viewport.height as f32) / 2.0 - y_from_top - line_height / 2.0 + border_thickness / 2.0;
            let bottom_translation = Vec3::new(border_center_x, bottom_y, -0.4);

            if let Some(&entity) = border_entities.get(&(idx, false)) {
                if let Ok((_, _, mut transform, mut sprite, mut visibility)) = border_query.get_mut(entity) {
                    transform.translation = bottom_translation;
                    sprite.custom_size = Some(Vec2::new(border_width, border_thickness));
                    sprite.color = border_color;
                    *visibility = Visibility::Visible;
                }
                border_entities.remove(&(idx, false));
            } else {
                commands.spawn((
                    Sprite {
                        color: border_color,
                        custom_size: Some(Vec2::new(border_width, border_thickness)),
                        ..default()
                    },
                    Transform::from_translation(bottom_translation),
                    Visibility::Visible,
                    CursorLineBorder { cursor_index: idx, is_top: false },
                    Name::new(format!("CursorLineBorder_bottom_{}", idx)),
                ));
            }
        }

        // === WORD HIGHLIGHT ===
        if !cursor_line_settings.highlight_word {
            continue;
        }
        // Find word boundaries at cursor position
        let line_start = state.rope.line_to_char(line_index);
        let col = cursor_pos - line_start;

        // Get the line text
        let line = state.rope.line(line_index);
        let line_chars: Vec<char> = line.chars().collect();

        // Check if cursor is on a word character (also check char before cursor if cursor is at end)
        let is_word_char = |c: char| c.is_alphanumeric() || c == '_';

        let on_word = if col < line_chars.len() && is_word_char(line_chars[col]) {
            true
        } else if col > 0 && col <= line_chars.len() && is_word_char(line_chars[col - 1]) {
            true
        } else {
            false
        };

        // Find word start and end
        let (word_start, word_end) = if on_word {
            // Find a valid starting position
            let start_col = if col < line_chars.len() && is_word_char(line_chars[col]) {
                col
            } else {
                col - 1
            };

            // Scan backwards for word start
            let mut ws = start_col;
            while ws > 0 && is_word_char(line_chars[ws - 1]) {
                ws -= 1;
            }

            // Scan forwards for word end
            let mut we = start_col;
            while we < line_chars.len() && is_word_char(line_chars[we]) {
                we += 1;
            }

            (ws, we)
        } else {
            (col, col)
        };

        // Only show word highlight if we found a word
        if word_end > word_start {
            let word_width = (word_end - word_start) as f32 * char_width;
            let word_x_left = settings.ui.layout.code_margin_left + (word_start as f32 * char_width);

            let word_center_x = -(viewport.width as f32) / 2.0 + word_x_left + word_width / 2.0 + viewport.offset_x - state.horizontal_scroll_offset;
            let word_center_y = (viewport.height as f32) / 2.0 - y_from_top;

            let word_translation = Vec3::new(word_center_x, word_center_y, -0.5);

            if let Some(&entity) = word_entities.get(&idx) {
                if let Ok((_, _, mut transform, mut sprite, mut visibility)) = word_query.get_mut(entity) {
                    transform.translation = word_translation;
                    sprite.custom_size = Some(Vec2::new(word_width, line_height));
                    sprite.color = word_highlight_color;
                    *visibility = Visibility::Visible;
                }
                word_entities.remove(&idx);
            } else {
                commands.spawn((
                    Sprite {
                        color: word_highlight_color,
                        custom_size: Some(Vec2::new(word_width, line_height)),
                        ..default()
                    },
                    Transform::from_translation(word_translation),
                    Visibility::Visible,
                    CursorWordHighlight { cursor_index: idx },
                    Name::new(format!("CursorWordHighlight_{}", idx)),
                ));
            }
        } else {
            // No word under cursor, hide word highlight
            if let Some(&entity) = word_entities.get(&idx) {
                if let Ok((_, _, _, _, mut visibility)) = word_query.get_mut(entity) {
                    *visibility = Visibility::Hidden;
                }
                word_entities.remove(&idx);
            }
        }
    }

    // Despawn excess entities
    for (_, entity) in border_entities {
        commands.entity(entity).despawn();
    }
    for (_, entity) in word_entities {
        commands.entity(entity).despawn();
    }
}

