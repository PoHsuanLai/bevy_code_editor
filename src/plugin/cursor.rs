//! Cursor rendering and animation

use super::editor_ui_plugin::EditorRenderConfig;
use super::to_bevy_coords_left_aligned;
use crate::settings::{
    CursorLineSettings, CursorSettings, FontSettings, IndentationSettings, ThemeSettings,
    WrappingSettings,
};
use crate::types::*;
use bevy::prelude::*;

pub struct CursorPlugin;

impl Plugin for CursorPlugin {
    fn build(&self, app: &mut App) {
        // Track cursor movement for blink reset (must run before cursor rendering)
        app.add_systems(Update, track_cursor_movement.in_set(super::ApplyStateSet));

        // Cursor rendering and animation
        app.add_systems(
            Update,
            (update_cursor, animate_cursor)
                .chain()
                .in_set(super::RenderingSet),
        );
        
        // Cursor line highlight
        app.add_systems(
            Update,
            update_cursor_line_highlight
                .in_set(super::RenderingSet),
        );
    }
}

/// Track when cursor position changes and update the timestamp for blink reset
/// Uses a separate field (last_cursor_pos_for_blink) to avoid race conditions
/// with auto_scroll_to_cursor which also uses last_cursor_pos
pub(crate) fn track_cursor_movement(mut state: ResMut<CodeEditorState>, time: Res<Time>) {
    // Check if cursor position has changed (use cursors[0].position for multi-cursor support)
    let current_pos = state.cursors.first().map(|c| c.position).unwrap_or(0);
    if current_pos != state.last_cursor_pos_for_blink {
        state.cursor_moved_time = time.elapsed_secs_f64();
        state.last_cursor_pos_for_blink = current_pos;
    }
}

pub(crate) fn update_cursor(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    font: Res<FontSettings>,
    cursor_settings: Res<CursorSettings>,
    theme: Res<ThemeSettings>,
    wrapping: Res<WrappingSettings>,
    indentation: Res<IndentationSettings>,
    viewport: Res<ViewportDimensions>,
    #[cfg(feature = "folding")]
    fold_state: Res<FoldState>,
    render_config: Res<EditorRenderConfig>,
    mut cursor_query: Query<(Entity, &EditorCursor, &mut Transform, &mut Visibility)>,
) {
    if !state.is_changed() {
        return;
    }

    let char_width = font.char_width;
    let line_height = font.line_height;
    let cursor_height = line_height * cursor_settings.height_multiplier;
    let cursor_count = state.cursors.len();

    // Check if we're using soft line wrapping
    let use_wrapping = wrapping.enabled && state.display_map.wrap_width > 0;

    // Collect existing cursor entities by their index
    let mut cursor_entities: std::collections::HashMap<usize, Entity> =
        std::collections::HashMap::new();
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
            #[cfg(feature = "folding")]
            let display_row = fold_state.actual_to_display_line(line_index);
            #[cfg(not(feature = "folding"))]
            let display_row = line_index;
            (display_row, col_index)
        };

        // For wrapped continuation rows, add indent offset
        let extra_indent = if use_wrapping && wrapping.indent_wrapped_lines {
            if state.display_map.is_continuation(display_row) {
                indentation.indent_size as f32 * char_width
            } else {
                0.0
            }
        } else {
            0.0
        };

        let x_offset = viewport.text_area_left + extra_indent + (display_col as f32 * char_width);
        let y_offset =
            viewport.text_area_top + state.scroll_offset + (display_row as f32 * line_height);

        // No horizontal scroll in wrapped mode
        let h_scroll = if use_wrapping {
            0.0
        } else {
            state.horizontal_scroll_offset
        };

        let translation = to_bevy_coords_left_aligned(
            x_offset,
            y_offset,
            viewport.width as f32,
            viewport.height as f32,
            viewport.offset_x,
            viewport.offset_y,
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
            let mut entity_cmd = commands.spawn((
                Sprite {
                    color: theme.cursor,
                    custom_size: Some(Vec2::new(cursor_settings.width, cursor_height)),
                    ..default()
                },
                Transform::from_translation(Vec3::new(translation.x, translation.y, 1.0)),
                Visibility::Visible,
                EditorCursor { cursor_index: idx },
                Name::new(format!("EditorCursor_{}", idx)),
            ));
            if let Some(ref layers) = render_config.render_layers {
                entity_cmd.insert(layers.clone());
            }
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
/// The cursor stays visible for a short period after movement before blinking resumes
pub(crate) fn animate_cursor(
    time: Res<Time>,
    cursor: Res<CursorSettings>,
    state: Res<CodeEditorState>,
    mut cursor_query: Query<&mut Visibility, With<EditorCursor>>,
) {
    if cursor.blink_rate == 0.0 {
        for mut visibility in cursor_query.iter_mut() {
            *visibility = Visibility::Visible;
        }
        return;
    }

    // Keep cursor visible for 0.5 seconds after movement before blinking
    let time_since_move = time.elapsed_secs_f64() - state.cursor_moved_time;
    let blink_pause_duration = 0.5; // seconds

    let new_visibility = if time_since_move < blink_pause_duration {
        // Cursor was recently moved - stay visible
        Visibility::Visible
    } else {
        // Resume blinking, starting from the time after the pause
        let blink_time = (time_since_move - blink_pause_duration) as f32;
        let blink_phase = (blink_time * cursor.blink_rate) % 1.0;
        if blink_phase < 0.5 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        }
    };

    for mut visibility in cursor_query.iter_mut() {
        *visibility = new_visibility;
    }
}
pub(crate) fn update_cursor_line_highlight(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    font: Res<FontSettings>,
    cursor_line: Res<CursorLineSettings>,
    theme: Res<ThemeSettings>,
    wrapping: Res<WrappingSettings>,
    _indentation: Res<IndentationSettings>,
    viewport: Res<ViewportDimensions>,
    #[cfg(feature = "folding")]
    fold_state: Res<FoldState>,
    render_config: Res<EditorRenderConfig>,
    mut border_query: Query<(
        Entity,
        &CursorLineBorder,
        &mut Transform,
        &mut Sprite,
        &mut Visibility,
    )>,
    mut word_query: Query<
        (
            Entity,
            &CursorWordHighlight,
            &mut Transform,
            &mut Sprite,
            &mut Visibility,
        ),
        Without<CursorLineBorder>,
    >,
) {
    // Skip if cursor line highlighting is disabled entirely
    if !cursor_line.enabled {
        // Hide all existing borders and word highlights
        for (_, _, _, _, mut visibility) in border_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        for (_, _, _, _, mut visibility) in word_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    // Get base highlight color from theme (unused for now, kept for future reference)
    let _base_highlight_color = match theme.line_highlight {
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

    let line_height = font.line_height;
    let char_width = font.char_width;
    let use_wrapping = wrapping.enabled && state.display_map.wrap_width > 0;

    // Border settings from configuration
    let border_thickness = cursor_line.border_thickness;
    let border_color = cursor_line.border_color;

    // Word highlight color from configuration
    let word_highlight_color = cursor_line.word_highlight_color;

    // Collect existing entities
    let mut border_entities: std::collections::HashMap<(usize, bool), Entity> =
        std::collections::HashMap::new();
    for (entity, border, _, _, _) in border_query.iter() {
        border_entities.insert((border.cursor_index, border.is_top), entity);
    }

    let mut word_entities: std::collections::HashMap<usize, Entity> =
        std::collections::HashMap::new();
    for (entity, word_hl, _, _, _) in word_query.iter() {
        word_entities.insert(word_hl.cursor_index, entity);
    }

    // Calculate border width (code area only, not the gutter)
    let code_area_start = viewport.text_area_left;
    let border_width = viewport.width as f32 - code_area_start;
    // Camera viewport handles panel positioning, so no offset_x here
    let border_center_x = viewport.world_left() + code_area_start + border_width / 2.0;

    // Process each cursor
    for (idx, cursor) in state.cursors.iter().enumerate() {
        let cursor_pos = cursor.position.min(state.rope.len_chars());
        let line_index = state.rope.char_to_line(cursor_pos);

        // Skip if line is hidden due to folding
        #[cfg(feature = "folding")]
        if fold_state.is_line_hidden(line_index) {
            continue;
        }

        // Calculate display row
        let display_row = if use_wrapping {
            state.display_map.buffer_to_display(line_index, 0).0
        } else {
            #[cfg(feature = "folding")]
            {
                let mut visible_row = line_index;
                for i in 0..line_index {
                    if fold_state.is_line_hidden(i) {
                        visible_row = visible_row.saturating_sub(1);
                    }
                }
                visible_row
            }
            #[cfg(not(feature = "folding"))]
            {
                line_index
            }
        };

        let y_from_top =
            viewport.text_area_top + state.scroll_offset + (display_row as f32 * line_height);

        // === TOP BORDER ===
        if cursor_line.show_border {
            let top_y = viewport.world_top() - y_from_top + line_height / 2.0
                - border_thickness / 2.0;
            let top_translation = Vec3::new(border_center_x, top_y, -0.4);

            if let Some(&entity) = border_entities.get(&(idx, true)) {
                if let Ok((_, _, mut transform, mut sprite, mut visibility)) =
                    border_query.get_mut(entity)
                {
                    transform.translation = top_translation;
                    sprite.custom_size = Some(Vec2::new(border_width, border_thickness));
                    sprite.color = border_color;
                    *visibility = Visibility::Visible;
                }
                border_entities.remove(&(idx, true));
            } else {
                let mut entity_cmd = commands.spawn((
                    Sprite {
                        color: border_color,
                        custom_size: Some(Vec2::new(border_width, border_thickness)),
                        ..default()
                    },
                    Transform::from_translation(top_translation),
                    Visibility::Visible,
                    CursorLineBorder {
                        cursor_index: idx,
                        is_top: true,
                    },
                    Name::new(format!("CursorLineBorder_top_{}", idx)),
                ));
                if let Some(ref layers) = render_config.render_layers {
                    entity_cmd.insert(layers.clone());
                }
            }

            // === BOTTOM BORDER ===
            let bottom_y = viewport.world_top() - y_from_top - line_height / 2.0
                + border_thickness / 2.0;
            let bottom_translation = Vec3::new(border_center_x, bottom_y, -0.4);

            if let Some(&entity) = border_entities.get(&(idx, false)) {
                if let Ok((_, _, mut transform, mut sprite, mut visibility)) =
                    border_query.get_mut(entity)
                {
                    transform.translation = bottom_translation;
                    sprite.custom_size = Some(Vec2::new(border_width, border_thickness));
                    sprite.color = border_color;
                    *visibility = Visibility::Visible;
                }
                border_entities.remove(&(idx, false));
            } else {
                let mut entity_cmd = commands.spawn((
                    Sprite {
                        color: border_color,
                        custom_size: Some(Vec2::new(border_width, border_thickness)),
                        ..default()
                    },
                    Transform::from_translation(bottom_translation),
                    Visibility::Visible,
                    CursorLineBorder {
                        cursor_index: idx,
                        is_top: false,
                    },
                    Name::new(format!("CursorLineBorder_bottom_{}", idx)),
                ));
                if let Some(ref layers) = render_config.render_layers {
                    entity_cmd.insert(layers.clone());
                }
            }
        }

        // === WORD HIGHLIGHT ===
        if !cursor_line.highlight_word {
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
        } else {
            col > 0 && col <= line_chars.len() && is_word_char(line_chars[col - 1])
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
            let word_x_left = viewport.text_area_left + (word_start as f32 * char_width);

            // Camera viewport handles panel positioning, so no offset_x here
            let word_center_x = viewport.world_left() + word_x_left + word_width / 2.0
                - state.horizontal_scroll_offset;
            let word_center_y = viewport.world_top() - y_from_top;

            let word_translation = Vec3::new(word_center_x, word_center_y, -0.5);

            if let Some(&entity) = word_entities.get(&idx) {
                if let Ok((_, _, mut transform, mut sprite, mut visibility)) =
                    word_query.get_mut(entity)
                {
                    transform.translation = word_translation;
                    sprite.custom_size = Some(Vec2::new(word_width, line_height));
                    sprite.color = word_highlight_color;
                    *visibility = Visibility::Visible;
                }
                word_entities.remove(&idx);
            } else {
                let mut entity_cmd = commands.spawn((
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
                if let Some(ref layers) = render_config.render_layers {
                    entity_cmd.insert(layers.clone());
                }
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
