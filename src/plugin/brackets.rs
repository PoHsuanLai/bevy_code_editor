//! Bracket matching and find highlights

use bevy::prelude::*;
use crate::settings::EditorSettings;
use crate::types::*;

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
    settings: Res<EditorSettings>,
    mut bracket_state: ResMut<BracketMatchState>,
) {
    // Only update when cursor moves or text changes
    if !state.is_changed() {
        return;
    }

    // Check if bracket matching is enabled
    if !settings.brackets.highlight_matching {
        bracket_state.current_match = None;
        return;
    }

    let cursor_pos = state.cursor_pos.min(state.rope.len_chars());
    bracket_state.current_match = find_matching_bracket(
        &state.rope,
        cursor_pos,
        &settings.brackets.pairs,
    );
}

/// Render bracket match highlights
pub(crate) fn update_bracket_highlight(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    bracket_state: Res<BracketMatchState>,
    fold_state: Res<FoldState>,
    mut highlight_query: Query<(Entity, &BracketMatchHighlight, &mut Transform, &mut Sprite, &mut Visibility)>,
) {
    let mut highlights: Vec<_> = highlight_query.iter_mut().collect();

    match &bracket_state.current_match {
        Some(bracket_match) => {
            let char_width = settings.font.char_width;
            let line_height = settings.font.line_height;
            let viewport_width = viewport.width as f32;
            let viewport_height = viewport.height as f32;
            let use_box_style = settings.brackets.use_box_style;
            let border_thickness = settings.brackets.box_border_thickness;

            // Calculate positions for both brackets
            let positions = [
                bracket_match.cursor_bracket_pos,
                bracket_match.matching_bracket_pos,
            ];

            let mut entity_index = 0;

            for (bracket_idx, &bracket_pos) in positions.iter().enumerate() {
                let line_idx = state.rope.char_to_line(bracket_pos);

                // Skip if line is hidden due to folding
                if fold_state.is_line_hidden(line_idx) {
                    continue;
                }

                let line_start = state.rope.line_to_char(line_idx);
                let col_idx = bracket_pos - line_start;

                // Calculate display row accounting for folded lines
                let display_row = fold_state.actual_to_display_line(line_idx);

                let x_offset = settings.ui.layout.code_margin_left + (col_idx as f32 * char_width);
                let y_offset = settings.ui.layout.margin_top + state.scroll_offset + (display_row as f32 * line_height);

                // Calculate base position (center of the bracket character cell)
                let base_x = -viewport_width / 2.0 + x_offset + char_width / 2.0 - state.horizontal_scroll_offset + viewport.offset_x;
                let base_y = viewport_height / 2.0 - y_offset;

                if use_box_style {
                    // Box style: 4 edges per bracket (top, bottom, left, right)
                    let edges = [
                        // (x_offset, y_offset, width, height) relative to base position
                        // Top edge
                        (0.0, line_height / 2.0 - border_thickness / 2.0, char_width, border_thickness),
                        // Bottom edge
                        (0.0, -line_height / 2.0 + border_thickness / 2.0, char_width, border_thickness),
                        // Left edge
                        (-char_width / 2.0 + border_thickness / 2.0, 0.0, border_thickness, line_height),
                        // Right edge
                        (char_width / 2.0 - border_thickness / 2.0, 0.0, border_thickness, line_height),
                    ];

                    for (edge_idx, (dx, dy, w, h)) in edges.iter().enumerate() {
                        let translation = Vec3::new(base_x + dx, base_y + dy, 0.4);
                        let size = Vec2::new(*w, *h);

                        if entity_index < highlights.len() {
                            // Reuse existing entity
                            let (_, _, ref mut transform, ref mut sprite, ref mut visibility) = &mut highlights[entity_index];
                            transform.translation = translation;
                            sprite.custom_size = Some(size);
                            sprite.color = settings.theme.bracket_match;
                            **visibility = Visibility::Visible;
                        } else {
                            // Spawn new edge entity
                            commands.spawn((
                                Sprite {
                                    color: settings.theme.bracket_match,
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
                        entity_index += 1;
                    }
                } else {
                    // Filled style: single sprite per bracket
                    let translation = Vec3::new(base_x, base_y, 0.4);
                    let size = Vec2::new(char_width, line_height);

                    if entity_index < highlights.len() {
                        // Reuse existing entity
                        let (_, _, ref mut transform, ref mut sprite, ref mut visibility) = &mut highlights[entity_index];
                        transform.translation = translation;
                        sprite.custom_size = Some(size);
                        sprite.color = settings.theme.bracket_match;
                        **visibility = Visibility::Visible;
                    } else {
                        // Spawn new highlight entity
                        commands.spawn((
                            Sprite {
                                color: settings.theme.bracket_match,
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

/// Render find/search match highlights
pub(crate) fn update_find_highlights(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    find_state: Res<FindState>,
    fold_state: Res<FoldState>,
    mut highlight_query: Query<(Entity, &FindHighlight, &mut Transform, &mut Sprite, &mut Visibility)>,
) {
    // If find is not active or no matches, hide all highlights
    if !find_state.active || find_state.matches.is_empty() {
        for (_, _, _, _, mut visibility) in highlight_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    let char_width = settings.font.char_width;
    let line_height = settings.font.line_height;
    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;

    // Calculate visible line range for culling (in display coordinates)
    let visible_start_row = ((-state.scroll_offset) / line_height).floor() as usize;
    let visible_lines = ((viewport_height / line_height).ceil() as usize) + 2;
    let visible_end_row = visible_start_row + visible_lines;

    // Collect existing highlight entities by match_index
    let mut existing_highlights: std::collections::HashMap<usize, Entity> = std::collections::HashMap::new();
    for (entity, highlight, _, _, _) in highlight_query.iter() {
        existing_highlights.insert(highlight.match_index, entity);
    }

    // Track which highlights we've updated
    let mut used_indices: std::collections::HashSet<usize> = std::collections::HashSet::new();

    // Update or create highlights for visible matches
    for (match_idx, find_match) in find_state.matches.iter().enumerate() {
        // Check if this match is visible
        let start_line = state.rope.char_to_line(find_match.start.min(state.rope.len_chars()));

        // Skip if line is hidden due to folding
        if fold_state.is_line_hidden(start_line) {
            continue;
        }

        // Calculate display row accounting for folded lines
        let display_row = fold_state.actual_to_display_line(start_line);

        // Skip if completely outside visible range (in display coordinates)
        if display_row < visible_start_row.saturating_sub(1) || display_row > visible_end_row {
            continue;
        }

        // Determine color based on whether this is the current match
        let is_current = find_state.current_match_index == Some(match_idx);
        let color = if is_current {
            settings.theme.find_match_current
        } else {
            settings.theme.find_match
        };

        // For simplicity, we'll highlight the entire match as a single rectangle on the first line
        // A more complete implementation would handle multi-line matches
        let line_start_char = state.rope.line_to_char(start_line);
        let start_col = find_match.start - line_start_char;
        let match_len = find_match.end - find_match.start;

        let x_offset = settings.ui.layout.code_margin_left + (start_col as f32 * char_width);
        let y_offset = settings.ui.layout.margin_top + state.scroll_offset + (display_row as f32 * line_height);

        // Calculate sprite position and size
        let sprite_width = match_len as f32 * char_width;
        let sprite_x = -viewport_width / 2.0 + x_offset + sprite_width / 2.0 - state.horizontal_scroll_offset + viewport.offset_x;
        let sprite_y = viewport_height / 2.0 - y_offset;
        let translation = Vec3::new(sprite_x, sprite_y, 0.3); // z=0.3 behind bracket highlights

        used_indices.insert(match_idx);

        if let Some(entity) = existing_highlights.get(&match_idx) {
            // Update existing highlight
            if let Ok((_, _, mut transform, mut sprite, mut visibility)) = highlight_query.get_mut(*entity) {
                transform.translation = translation;
                sprite.color = color;
                sprite.custom_size = Some(Vec2::new(sprite_width, line_height));
                *visibility = Visibility::Visible;
            }
        } else {
            // Spawn new highlight entity
            commands.spawn((
                Sprite {
                    color,
                    custom_size: Some(Vec2::new(sprite_width, line_height)),
                    ..default()
                },
                Transform::from_translation(translation),
                FindHighlight { match_index: match_idx },
                Name::new(format!("FindHighlight_{}", match_idx)),
                Visibility::Visible,
            ));
        }
    }

    // Hide unused highlights
    for (entity, highlight, _, _, mut visibility) in highlight_query.iter_mut() {
        if !used_indices.contains(&highlight.match_index) {
            *visibility = Visibility::Hidden;
        }
        // Also clean up highlights that are for indices beyond the current match count
        if highlight.match_index >= find_state.matches.len() {
            commands.entity(entity).despawn();
        }
    }
}
