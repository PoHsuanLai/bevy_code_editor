//! Cursor rendering and animation
//!
//! As of step 6a, cursor carets are pushed into `TextViewOverlays` as `RectOverlay`s
//! rather than spawned as separate `Sprite` entities. Blink folds into `update_cursor`
//! (no separate `animate_cursor` system).

use crate::settings::{
    CursorLineSettings, CursorSettings, FontSettings, IndentationSettings, ThemeSettings,
    WrappingSettings,
};
use crate::text_view::{RectOverlay, TextViewOverlays, TextViewState, TextViewViewport};
use crate::types::*;
use bevy::prelude::*;

#[allow(unused_imports)]
use super::editor_ui_plugin::EditorRenderConfig;

pub struct CursorPlugin;

impl Plugin for CursorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, track_cursor_movement.in_set(super::ApplyStateSet));

        // update_cursor runs in OverlaySet (between RenderingSet's display map build
        // and the actual render). For now we keep it in RenderingSet pending step 9's
        // explicit OverlaySet introduction.
        app.add_systems(Update, push_cursor_overlays.in_set(super::RenderingSet));

        // Note: update_cursor_line_highlight is registered by EditorUiPlugin
        // where it's chained with other visual systems.
    }
}

/// Uses last_cursor_pos_for_blink (separate from last_cursor_pos) to avoid
/// race conditions with auto_scroll_to_cursor
pub(crate) fn track_cursor_movement(
    mut editor_query: Query<&mut CursorState, With<CodeEditor>>,
    time: Res<Time>,
) {
    let Ok(mut cursor) = editor_query.single_mut() else {
        return;
    };
    let current_pos = cursor.cursors.first().map(|c| c.position).unwrap_or(0);
    if current_pos != cursor.last_cursor_pos_for_blink {
        cursor.cursor_moved_time = time.elapsed_secs_f64();
        cursor.last_cursor_pos_for_blink = current_pos;
    }
}

/// Push caret rectangles into `TextViewOverlays` for each cursor.
///
/// Replaces the previous `update_cursor` + `animate_cursor` pair: blink and
/// position now collapse into one system that simply skips pushing during
/// the off-phase of the blink cycle.
///
/// Note: this system is a *partial* writer of `TextViewOverlays` — it pushes
/// caret rects without clearing first, since selection (step 6b) and other
/// overlay producers also push. A future `OverlaySet` will introduce a single
/// `clear_overlays` system that runs first; until then we drain previous
/// caret rects ourselves.
pub(crate) fn push_cursor_overlays(
    editor_query: Query<
        (
            &EditorDisplayState,
            &CursorState,
            &TextViewState,
            &TextViewViewport,
            &mut TextViewOverlays,
        ),
        With<CodeEditor>,
    >,
    font: Res<FontSettings>,
    cursor_settings: Res<CursorSettings>,
    theme: Res<ThemeSettings>,
    wrapping: Res<WrappingSettings>,
    indentation: Res<IndentationSettings>,
    #[cfg(feature = "folding")] fold_state: Res<FoldState>,
    time: Res<Time>,
) {
    let mut iter = editor_query;
    let Ok((display, cursor, tv, _vp, mut overlays)) = iter.single_mut() else {
        return;
    };

    // Drain any caret rects from the previous frame. We mark them with z=+1 so
    // we can identify them; selection rects use z=-1 (added in step 6b).
    overlays.rects.retain(|r| r.z != 1);

    // Blink: skip pushing during the off-phase. Always visible for half a second
    // after movement, then alternates at `blink_rate` Hz.
    let blink_visible = if cursor_settings.blink_rate == 0.0 {
        true
    } else {
        let time_since_move = time.elapsed_secs_f64() - cursor.cursor_moved_time;
        let blink_pause_duration = 0.5;
        if time_since_move < blink_pause_duration {
            true
        } else {
            let blink_time = (time_since_move - blink_pause_duration) as f32;
            let blink_phase = (blink_time * cursor_settings.blink_rate) % 1.0;
            blink_phase < 0.5
        }
    };
    if !blink_visible {
        overlays.version = overlays.version.wrapping_add(1);
        return;
    }

    let char_width = font.char_width;
    let use_wrapping = wrapping.enabled && display.display_map.wrap_width > 0;

    for c in cursor.cursors.iter() {
        let cursor_pos = c.position.min(tv.rope.len_chars());
        let line_index = tv.rope.char_to_line(cursor_pos);
        let line_start = tv.rope.line_to_char(line_index);
        let col_index = cursor_pos - line_start;

        let (display_row, display_col) = if use_wrapping {
            display.display_map.buffer_to_display(line_index, col_index)
        } else {
            #[cfg(feature = "folding")]
            let display_row = fold_state.actual_to_display_line(line_index);
            #[cfg(not(feature = "folding"))]
            let display_row = line_index;
            (display_row, col_index)
        };

        let extra_indent = if use_wrapping && wrapping.indent_wrapped_lines {
            if display.display_map.is_continuation(display_row) {
                indentation.indent_size as f32 * char_width
            } else {
                0.0
            }
        } else {
            0.0
        };

        let x_left = extra_indent + (display_col as f32 * char_width);
        let x_right = x_left + cursor_settings.width;

        overlays.rects.push(RectOverlay {
            display_row: display_row as u32,
            x_range: x_left..x_right,
            color: theme.cursor,
            z: 1, // above text
            corner_radius: 0.0,
        });
    }

    overlays.version = overlays.version.wrapping_add(1);
}
pub(crate) fn update_cursor_line_highlight(
    mut commands: Commands,
    editor_query: Query<
        (
            &CodeEditorState,
            &EditorDisplayState,
            &CursorState,
            &TextViewState,
            &TextViewViewport,
        ),
        With<CodeEditor>,
    >,
    font: Res<FontSettings>,
    cursor_line: Res<CursorLineSettings>,
    theme: Res<ThemeSettings>,
    wrapping: Res<WrappingSettings>,
    _indentation: Res<IndentationSettings>,
    #[cfg(feature = "folding")] fold_state: Res<FoldState>,
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
    if !cursor_line.enabled {
        for (_, _, _, _, mut visibility) in border_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        for (_, _, _, _, mut visibility) in word_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    let _base_highlight_color = match theme.line_highlight {
        Some(color) => color,
        None => {
            for (_, _, _, _, mut visibility) in border_query.iter_mut() {
                *visibility = Visibility::Hidden;
            }
            for (_, _, _, _, mut visibility) in word_query.iter_mut() {
                *visibility = Visibility::Hidden;
            }
            return;
        }
    };

    let Ok((_editor, display, cursor, tv, vp)) = editor_query.single() else {
        return;
    };

    let line_height = font.line_height;
    let char_width = font.char_width;
    let use_wrapping = wrapping.enabled && display.display_map.wrap_width > 0;

    let border_thickness = cursor_line.border_thickness;
    let border_color = cursor_line.border_color;
    let word_highlight_color = cursor_line.word_highlight_color;

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

    let code_area_start = vp.text_area_left;
    let border_width = vp.width as f32 - code_area_start;
    // Camera viewport handles panel positioning, so no offset_x here
    let border_center_x = vp.world_left() + code_area_start + border_width / 2.0;

    for (idx, c) in cursor.cursors.iter().enumerate() {
        let cursor_pos = c.position.min(tv.rope.len_chars());
        let line_index = tv.rope.char_to_line(cursor_pos);

        #[cfg(feature = "folding")]
        if fold_state.is_line_hidden(line_index) {
            continue;
        }

        let display_row = if use_wrapping {
            display.display_map.buffer_to_display(line_index, 0).0
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

        let y_from_top = vp.text_area_top + tv.scroll_offset + (display_row as f32 * line_height);

        if cursor_line.show_border {
            let top_y = vp.world_top() - y_from_top + line_height / 2.0 - border_thickness / 2.0;
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

            let bottom_y = vp.world_top() - y_from_top - line_height / 2.0 + border_thickness / 2.0;
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
        let line_start = tv.rope.line_to_char(line_index);
        let col = cursor_pos - line_start;

        // Get the line text
        let line = tv.rope.line(line_index);
        let line_chars: Vec<char> = line.chars().collect();

        // Check if cursor is on a word character (also check char before cursor if cursor is at end)
        let is_word_char = |ch: char| ch.is_alphanumeric() || ch == '_';

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
            let word_x_left = vp.text_area_left + (word_start as f32 * char_width);

            // Camera viewport handles panel positioning, so no offset_x here
            let word_center_x =
                vp.world_left() + word_x_left + word_width / 2.0 - tv.horizontal_scroll_offset;
            let word_center_y = vp.world_top() - y_from_top;

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
