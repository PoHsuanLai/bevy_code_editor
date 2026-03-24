//! Bracket matching and find highlights
#![allow(dead_code)]

use super::editor_ui_plugin::EditorRenderConfig;
use crate::settings::*;
use crate::types::*;
use bevy::prelude::*;

pub struct BracketPlugin;

impl Plugin for BracketPlugin {
    fn build(&self, _app: &mut App) {
        #[cfg(feature = "brackets")]
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

    // Check if cursor is on a bracket
    // First check opening brackets
    for &(open, close) in bracket_pairs {
        if char_at_cursor == open {
            // Find matching closing bracket
            if let Some(match_pos) = find_closing_bracket(rope, pos, open, close) {
                return Some(BracketMatch {
                    cursor_bracket_pos: pos,
                    matching_bracket_pos: match_pos,
                });
            }
        } else if char_at_cursor == close {
            // Find matching opening bracket
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

/// Find matching closing bracket, handling nesting
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

/// Find matching opening bracket, handling nesting
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

/// Update bracket match state based on cursor position
pub(crate) fn update_bracket_match(
    state: Res<CodeEditorState>,
    brackets: Res<BracketSettings>,
    mut bracket_state: ResMut<BracketMatchState>,
) {
    // Only update when cursor moves or text changes
    if !state.is_changed() {
        return;
    }

    // Check if bracket matching is enabled
    if !brackets.enabled {
        bracket_state.current_match = None;
        return;
    }

    let cursor_pos = state.cursor_pos.min(state.rope.len_chars());
    bracket_state.current_match = find_matching_bracket(&state.rope, cursor_pos, &brackets.pairs);
}

/// Render bracket match highlights
pub(crate) fn update_bracket_highlight(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    font: Res<FontSettings>,
    theme: Res<ThemeSettings>,
    brackets: Res<BracketSettings>,
    viewport: Res<ViewportDimensions>,
    bracket_state: Res<BracketMatchState>,
    #[cfg(feature = "folding")]
    fold_state: Res<FoldState>,
    render_config: Res<EditorRenderConfig>,
    mut highlight_query: Query<(
        Entity,
        &BracketMatchHighlight,
        &mut Transform,
        &mut Sprite,
        &mut Visibility,
    )>,
) {
    let mut highlights: Vec<_> = highlight_query.iter_mut().collect();

    match &bracket_state.current_match {
        Some(bracket_match) => {
            let char_width = font.char_width;
            let line_height = font.line_height;
            let _viewport_width = viewport.width as f32;
            let _viewport_height = viewport.height as f32;
            let use_box_style = matches!(brackets.style, BracketHighlightStyle::Background)
                || matches!(brackets.style, BracketHighlightStyle::Both);
            let border_thickness = 2.0; // Default thickness

            // Calculate positions for both brackets
            let positions = [
                bracket_match.cursor_bracket_pos,
                bracket_match.matching_bracket_pos,
            ];

            let mut entity_index = 0;

            for (bracket_idx, &bracket_pos) in positions.iter().enumerate() {
                let line_idx = state.rope.char_to_line(bracket_pos);

                // Skip if line is hidden due to folding
                #[cfg(feature = "folding")]
                if fold_state.is_line_hidden(line_idx) {
                    continue;
                }

                let line_start = state.rope.line_to_char(line_idx);
                let col_idx = bracket_pos - line_start;

                // Calculate display row accounting for folded lines
                #[cfg(feature = "folding")]
                let display_row = fold_state.actual_to_display_line(line_idx);
                #[cfg(not(feature = "folding"))]
                let display_row = line_idx;

                let x_offset = viewport.text_area_left + (col_idx as f32 * char_width);
                let y_offset = viewport.text_area_top
                    + state.scroll_offset
                    + (display_row as f32 * line_height);

                // Calculate base position (center of the bracket character cell)
                // Camera viewport handles panel positioning, so no offset_x here
                let base_x = viewport.world_left() + x_offset + char_width / 2.0
                    - state.horizontal_scroll_offset;
                let base_y = viewport.world_top() - y_offset;

                if use_box_style {
                    // Box style: 4 edges per bracket (top, bottom, left, right)
                    let edges = [
                        // (x_offset, y_offset, width, height) relative to base position
                        // Top edge
                        (
                            0.0,
                            line_height / 2.0 - border_thickness / 2.0,
                            char_width,
                            border_thickness,
                        ),
                        // Bottom edge
                        (
                            0.0,
                            -line_height / 2.0 + border_thickness / 2.0,
                            char_width,
                            border_thickness,
                        ),
                        // Left edge
                        (
                            -char_width / 2.0 + border_thickness / 2.0,
                            0.0,
                            border_thickness,
                            line_height,
                        ),
                        // Right edge
                        (
                            char_width / 2.0 - border_thickness / 2.0,
                            0.0,
                            border_thickness,
                            line_height,
                        ),
                    ];

                    for (edge_idx, (dx, dy, w, h)) in edges.iter().enumerate() {
                        let translation = Vec3::new(base_x + dx, base_y + dy, 0.4);
                        let size = Vec2::new(*w, *h);

                        if entity_index < highlights.len() {
                            // Reuse existing entity
                            let (_, _, ref mut transform, ref mut sprite, ref mut visibility) =
                                &mut highlights[entity_index];
                            transform.translation = translation;
                            sprite.custom_size = Some(size);
                            sprite.color = theme.bracket_match;
                            **visibility = Visibility::Visible;
                        } else {
                            // Spawn new edge entity
                            let mut entity_cmd = commands.spawn((
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
                            if let Some(ref layers) = render_config.render_layers {
                                entity_cmd.insert(layers.clone());
                            }
                        }
                        entity_index += 1;
                    }
                } else {
                    // Filled style: single sprite per bracket
                    let translation = Vec3::new(base_x, base_y, 0.4);
                    let size = Vec2::new(char_width, line_height);

                    if entity_index < highlights.len() {
                        // Reuse existing entity
                        let (_, _, ref mut transform, ref mut sprite, ref mut visibility) =
                            &mut highlights[entity_index];
                        transform.translation = translation;
                        sprite.custom_size = Some(size);
                        sprite.color = theme.bracket_match;
                        **visibility = Visibility::Visible;
                    } else {
                        // Spawn new highlight entity
                        let mut entity_cmd = commands.spawn((
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
                        if let Some(ref layers) = render_config.render_layers {
                            entity_cmd.insert(layers.clone());
                        }
                    }
                    entity_index += 1;
                }
            }

            // Hide any extra highlight entities
            for i in entity_index..highlights.len() {
                let (_, _, _, _, ref mut visibility) = &mut highlights[i];
                **visibility = Visibility::Hidden;
            }
        }
        None => {
            // Hide all bracket highlights
            for (_, _, _, _, mut visibility) in highlight_query.iter_mut() {
                *visibility = Visibility::Hidden;
            }
        }
    }
}
