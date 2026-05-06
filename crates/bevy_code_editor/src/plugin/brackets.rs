//! Bracket matching and find highlights
#![allow(dead_code)]

use crate::settings::*;
use crate::text_view::{ScrollState, TextBuffer, TextViewViewport};
use crate::types::*;
use bevy::prelude::*;
use bevy_text_engine::FontConfig;

pub struct BracketPlugin;

impl Plugin for BracketPlugin {
    fn build(&self, _app: &mut App) {
        _app.add_systems(
            Update,
            (
                update_bracket_match.in_set(super::ApplyStateSet),
                update_bracket_highlight
                    .after(super::ui_elements::update_indent_guides)
                    .in_set(super::RenderingSet),
            ),
        );
    }
}

pub(crate) fn find_matching_bracket(
    rope: &ropey::Rope,
    pos: usize,
    bracket_pairs: &[(char, char)],
) -> Option<BracketMatch> {
    if pos >= rope.len_chars() {
        return None;
    }

    let char_at_cursor = rope.char(pos);

    for &(open, close) in bracket_pairs {
        if char_at_cursor == open {
            if let Some(match_pos) = find_closing_bracket(rope, pos, open, close) {
                return Some(BracketMatch {
                    cursor_bracket_pos: pos,
                    matching_bracket_pos: match_pos,
                });
            }
        } else if char_at_cursor == close {
            if let Some(match_pos) = find_opening_bracket(rope, pos, open, close) {
                return Some(BracketMatch {
                    cursor_bracket_pos: pos,
                    matching_bracket_pos: match_pos,
                });
            }
        }
    }

    // Also check character before cursor (common UX pattern)
    if pos > 0 {
        let char_before = rope.char(pos - 1);
        for &(open, close) in bracket_pairs {
            if char_before == open {
                if let Some(match_pos) = find_closing_bracket(rope, pos - 1, open, close) {
                    return Some(BracketMatch {
                        cursor_bracket_pos: pos - 1,
                        matching_bracket_pos: match_pos,
                    });
                }
            } else if char_before == close {
                if let Some(match_pos) = find_opening_bracket(rope, pos - 1, open, close) {
                    return Some(BracketMatch {
                        cursor_bracket_pos: pos - 1,
                        matching_bracket_pos: match_pos,
                    });
                }
            }
        }
    }

    None
}

pub(crate) fn find_closing_bracket(
    rope: &ropey::Rope,
    start_pos: usize,
    open: char,
    close: char,
) -> Option<usize> {
    let mut depth = 1;
    let mut pos = start_pos + 1;
    let len = rope.len_chars();

    while pos < len && depth > 0 {
        let c = rope.char(pos);
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(pos);
            }
        }
        pos += 1;
    }

    None
}

pub(crate) fn find_opening_bracket(
    rope: &ropey::Rope,
    start_pos: usize,
    open: char,
    close: char,
) -> Option<usize> {
    let mut depth = 1;
    let mut pos = start_pos;

    while pos > 0 && depth > 0 {
        pos -= 1;
        let c = rope.char(pos);
        if c == close {
            depth += 1;
        } else if c == open {
            depth -= 1;
            if depth == 0 {
                return Some(pos);
            }
        }
    }

    None
}

pub(crate) fn update_bracket_match(
    mut editor_query: Query<
        (&CursorState, &TextBuffer, &mut BracketMatchState),
        With<CodeEditor>,
    >,
    brackets: Res<BracketSettings>,
) {
    for (cursor, buffer, mut bracket_state) in editor_query.iter_mut() {
        if !brackets.enabled {
            bracket_state.current_match = None;
            continue;
        }

        let cursor_pos = cursor.cursor_pos.min(buffer.rope.len_chars());
        bracket_state.current_match = find_matching_bracket(&buffer.rope, cursor_pos, &brackets.pairs);
    }
}

pub(crate) fn update_bracket_highlight(
    mut commands: Commands,
    editor_query: Query<
        (
            &TextBuffer,
            &ScrollState,
            &TextViewViewport,
            &BracketMatchState,
            &FoldState,
            &FontConfig,
            Option<&crate::text_view::DisplayLayout>,
            &ThemeConfig,
        ),
        With<CodeEditor>,
    >,
    brackets: Res<BracketSettings>,
    mut highlight_query: Query<(
        Entity,
        &BracketMatchHighlight,
        &mut Transform,
        &mut Sprite,
        &mut Visibility,
    )>,
) {
    let mut highlights: Vec<_> = highlight_query.iter_mut().collect();
    let mut entity_index_global: usize = 0;
    let mut any_match = false;

    for (buffer, scroll, viewport, bracket_state, fold_state, font, layout, theme) in editor_query.iter() {
    match &bracket_state.current_match {
        Some(bracket_match) => {
            any_match = true;
            let char_width = font.char_width;
            let line_height = font.line_height;
            let _viewport_width = viewport.width as f32;
            let _viewport_height = viewport.height as f32;
            let use_box_style = matches!(brackets.style, BracketHighlightStyle::Background)
                || matches!(brackets.style, BracketHighlightStyle::Both);
            let border_thickness = 2.0; // Default thickness

            let positions = [
                bracket_match.cursor_bracket_pos,
                bracket_match.matching_bracket_pos,
            ];

            for (bracket_idx, &bracket_pos) in positions.iter().enumerate() {
                let line_idx = buffer.rope.char_to_line(bracket_pos);

                if fold_state.is_line_hidden(line_idx) {
                    continue;
                }

                let line_start = buffer.rope.line_to_char(line_idx);
                let col_idx = bracket_pos - line_start;

                // Pixel x of the bracket glyph and its visual width. With wrap
                // on, the bracket might be on a soft-wrap continuation row, so
                // resolve (display_row, byte_in_row) via the layout. Falls
                // back to fold-state + char_width math when off-viewport.
                let line = buffer.rope.line(line_idx);
                let col_clamped = col_idx.min(line.len_chars());
                let next_col = (col_idx + 1).min(line.len_chars());
                let start_byte = line.slice(..col_clamped).len_bytes();
                let end_byte = line.slice(..next_col).len_bytes();
                let (start_row, start_byte_in_row) = layout
                    .and_then(|l| l.buffer_to_display(line_idx as u32, start_byte))
                    .unwrap_or_else(|| (fold_state.actual_to_display_line(line_idx) as u32, start_byte));
                let (end_row, end_byte_in_row) = layout
                    .and_then(|l| l.buffer_to_display(line_idx as u32, end_byte))
                    .unwrap_or((start_row, end_byte));
                let display_row = start_row as usize;
                let glyph_x = layout
                    .and_then(|l| l.x_at_byte(start_row, start_byte_in_row))
                    .unwrap_or(col_idx as f32 * char_width);
                // If the bracket happens to be the last glyph before a soft
                // break, end_row may differ — use start-row width as the
                // fallback.
                let glyph_width = if start_row == end_row {
                    layout
                        .and_then(|l| {
                            let s = l.x_at_byte(start_row, start_byte_in_row)?;
                            let e = l.x_at_byte(end_row, end_byte_in_row)?;
                            Some((e - s).max(0.0))
                        })
                        .unwrap_or(char_width)
                } else {
                    char_width
                };

                let x_offset = viewport.text_area_left + glyph_x;
                let y_offset =
                    viewport.text_area_top + scroll.scroll_offset + (display_row as f32 * line_height);

                // Calculate base position (center of the bracket character cell)
                // Camera viewport handles panel positioning, so no offset_x here
                let base_x = viewport.world_left() + x_offset + glyph_width / 2.0
                    - scroll.horizontal_scroll_offset;
                let base_y = viewport.world_top() - y_offset;

                if use_box_style {
                    // Box style: 4 edges per bracket (top, bottom, left, right)
                    let edges = [
                        // (x_offset, y_offset, width, height) relative to base position
                        // Top edge
                        (
                            0.0,
                            line_height / 2.0 - border_thickness / 2.0,
                            glyph_width,
                            border_thickness,
                        ),
                        // Bottom edge
                        (
                            0.0,
                            -line_height / 2.0 + border_thickness / 2.0,
                            glyph_width,
                            border_thickness,
                        ),
                        // Left edge
                        (
                            -glyph_width / 2.0 + border_thickness / 2.0,
                            0.0,
                            border_thickness,
                            line_height,
                        ),
                        // Right edge
                        (
                            glyph_width / 2.0 - border_thickness / 2.0,
                            0.0,
                            border_thickness,
                            line_height,
                        ),
                    ];

                    for (edge_idx, (dx, dy, w, h)) in edges.iter().enumerate() {
                        let translation = Vec3::new(base_x + dx, base_y + dy, 0.4);
                        let size = Vec2::new(*w, *h);

                        if entity_index_global < highlights.len() {
                            let (_, _, ref mut transform, ref mut sprite, ref mut visibility) =
                                &mut highlights[entity_index_global];
                            transform.translation = translation;
                            sprite.custom_size = Some(size);
                            sprite.color = theme.bracket_match;
                            **visibility = Visibility::Visible;
                        } else {
                            commands.spawn((
                                Sprite {
                                    color: theme.bracket_match,
                                    custom_size: Some(size),
                                    ..default()
                                },
                                Transform::from_translation(translation),
                                BracketMatchHighlight {
                                    bracket_index: bracket_idx,
                                    edge: edge_idx,
                                },
                                Name::new(format!("BracketHighlight_{}_{}", bracket_idx, edge_idx)),
                                Visibility::Visible,
                            ));
                        }
                        entity_index_global += 1;
                    }
                } else {
                    // Filled style: single sprite per bracket
                    let translation = Vec3::new(base_x, base_y, 0.4);
                    let size = Vec2::new(glyph_width, line_height);

                    if entity_index_global < highlights.len() {
                        let (_, _, ref mut transform, ref mut sprite, ref mut visibility) =
                            &mut highlights[entity_index_global];
                        transform.translation = translation;
                        sprite.custom_size = Some(size);
                        sprite.color = theme.bracket_match;
                        **visibility = Visibility::Visible;
                    } else {
                        commands.spawn((
                            Sprite {
                                color: theme.bracket_match,
                                custom_size: Some(size),
                                ..default()
                            },
                            Transform::from_translation(translation),
                            BracketMatchHighlight {
                                bracket_index: bracket_idx,
                                edge: 0,
                            },
                            Name::new(format!("BracketHighlight_{}", bracket_idx)),
                            Visibility::Visible,
                        ));
                    }
                    entity_index_global += 1;
                }
            }
        }
        None => {}
    }
    }

    // After processing all editors, hide any leftover highlights
    if any_match {
        for i in entity_index_global..highlights.len() {
            let (_, _, _, _, ref mut visibility) = &mut highlights[i];
            **visibility = Visibility::Hidden;
        }
    } else {
        for (_, _, _, _, ref mut visibility) in highlights.iter_mut() {
            **visibility = Visibility::Hidden;
        }
    }
}
