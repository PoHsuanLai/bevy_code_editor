//! UI elements: line numbers, selection, indent guides

use bevy::prelude::*;
use crate::settings::EditorSettings;
use crate::types::*;
use super::to_bevy_coords_left_aligned;

pub(crate) fn update_line_numbers(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    fold_state: Res<FoldState>,
    mut line_numbers_query: Query<(&mut Text2d, &mut Transform, &mut Visibility, &mut TextColor), With<LineNumbers>>,
) {
    // Hide all line numbers if disabled in settings
    if !settings.ui.show_line_numbers {
        for (_, _, mut visibility, _) in line_numbers_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    if !state.is_changed() && !fold_state.is_changed() {
        return;
    }

    let line_height = settings.font.line_height;
    let font_size = settings.font.size;

    // Collect cursor lines for highlighting active line numbers
    let cursor_lines: std::collections::HashSet<usize> = state
        .cursors
        .iter()
        .map(|c| {
            let pos = c.position.min(state.rope.len_chars());
            state.rope.char_to_line(pos)
        })
        .collect();

    // Check if we're using soft line wrapping
    let use_wrapping = settings.wrapping.enabled && state.display_map.wrap_width > 0;

    // Use configurable buffer for viewport calculations
    let buffer_lines = settings.performance.viewport_buffer_lines as f32;
    let viewport_top = -state.scroll_offset - line_height * buffer_lines;
    let viewport_bottom = viewport_top + viewport.height as f32 + line_height * buffer_lines * 2.0;

    let first_visible_display_row =
        ((viewport_top - settings.ui.layout.margin_top) / line_height).floor().max(0.0) as usize;
    let last_visible_display_row =
        ((viewport_bottom - settings.ui.layout.margin_top) / line_height).ceil() as usize;

    let total_buffer_lines = state.line_count();

    let mut existing_line_numbers: Vec<_> = line_numbers_query.iter_mut().collect();
    let mut entity_index = 0;

    // === OPTIMIZATION: Skip to visible start instead of iterating from 0 ===
    let has_folding = !fold_state.regions.is_empty();

    // Calculate starting buffer line and display row
    let (start_buffer_line, mut current_display_row) = if has_folding {
        // With folding, we need to iterate to find the right buffer line
        let mut display_row = 0;
        let mut buffer_line = 0;
        while buffer_line < total_buffer_lines && display_row < first_visible_display_row {
            if !fold_state.is_line_hidden(buffer_line) {
                display_row += 1;
            }
            buffer_line += 1;
        }
        (buffer_line, display_row)
    } else {
        // No folding: display_row == buffer_line, jump directly
        let start = first_visible_display_row.min(total_buffer_lines);
        (start, start)
    };

    // Iterate over buffer lines starting from visible area
    for buffer_line in start_buffer_line..total_buffer_lines {
        // Skip lines that are hidden due to folding
        if fold_state.is_line_hidden(buffer_line) {
            continue;
        }

        // For wrapped mode, handle continuation rows
        let is_continuation = if use_wrapping {
            // In wrapped mode, we need to check if this display row is a continuation
            // For now, we'll use the simpler approach without wrapping for folded content
            false
        } else {
            false
        };

        // All lines from start_buffer_line should be in or after visible range
        if current_display_row <= last_visible_display_row {
            // Calculate Y position based on display row (not buffer line)
            let y = settings.ui.layout.margin_top + state.scroll_offset + (current_display_row as f32 * line_height);
            let translation = to_bevy_coords_left_aligned(
                settings.ui.layout.line_number_margin_left,
                y,
                viewport.width as f32,
                viewport.height as f32,
                viewport.offset_x,
                0.0,  // line numbers don't scroll horizontally
            );

            // For continuation rows, show empty or continuation indicator
            let line_number_text = if is_continuation {
                // Show nothing or a continuation indicator for wrapped lines
                String::new()
            } else {
                // Show actual buffer line number (1-indexed)
                (buffer_line + 1).to_string()
            };

            // Use active color for cursor lines
            let line_color = if cursor_lines.contains(&buffer_line) {
                settings.theme.line_numbers_active
            } else {
                settings.theme.line_numbers
            };

            if entity_index < existing_line_numbers.len() {
                let (ref mut text, ref mut transform, ref mut visibility, ref mut text_color) =
                    &mut existing_line_numbers[entity_index];
                text.0 = line_number_text;
                transform.translation = translation;
                text_color.0 = line_color;
                **visibility = Visibility::Visible;
            } else {
                let text_font = TextFont {
                    font: settings.font.handle.clone().unwrap_or_default(),
                    font_size,
                    ..default()
                };

                commands.spawn((
                    Text2d::new(line_number_text),
                    text_font,
                    TextColor(line_color),
                    Transform::from_translation(translation),
                    LineNumbers,
                    Name::new(format!("LineNumber_buffer_{}", buffer_line)),
                    Visibility::Visible,
                ));
            }

            entity_index += 1;
        }

        current_display_row += 1;

        // Early exit if we've passed the visible area
        if current_display_row > last_visible_display_row {
            break;
        }
    }

    // Hide unused line numbers
    for i in entity_index..existing_line_numbers.len() {
        let (_, _, ref mut visibility, _) = &mut existing_line_numbers[i];
        **visibility = Visibility::Hidden;
    }
}

/// Update selection highlight rectangles for all cursors
pub(crate) fn update_selection_highlight(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    fold_state: Res<FoldState>,
    mut selection_query: Query<(
        Entity,
        &mut Transform,
        &mut Sprite,
        &mut Visibility,
        &mut SelectionHighlight,
    )>,
) {
    if !state.is_changed() {
        return;
    }

    let char_width = settings.font.char_width;
    let line_height = settings.font.line_height;

    // Check if we're using soft line wrapping
    let use_wrapping = settings.wrapping.enabled && state.display_map.wrap_width > 0;

    // Collect all selection ranges from all cursors
    // (cursor_idx, display_row, start_col, end_col, is_continuation)
    let mut selection_rects: Vec<(usize, usize, usize, usize, bool)> = Vec::new();

    for (cursor_idx, cursor) in state.cursors.iter().enumerate() {
        if let Some((start, end)) = cursor.selection_range() {
            if start == end {
                continue;
            }

            let start_line = state.rope.char_to_line(start);
            let end_line = state.rope.char_to_line(end);

            for line_idx in start_line..=end_line {
                // Skip hidden lines
                if fold_state.is_line_hidden(line_idx) {
                    continue;
                }

                let line_start_char = state.rope.line_to_char(line_idx);
                let line = state.rope.line(line_idx);

                let sel_start_in_line = if line_idx == start_line {
                    start - line_start_char
                } else {
                    0
                };

                let sel_end_in_line = if line_idx == end_line {
                    end - line_start_char
                } else {
                    line.len_chars()
                };

                if sel_start_in_line < sel_end_in_line {
                    if use_wrapping {
                        // For wrapped mode, split selection across display rows
                        for (row_idx, row) in state.display_map.rows.iter().enumerate() {
                            if row.buffer_line != line_idx {
                                continue;
                            }
                            // Calculate overlap between selection and this row
                            let row_sel_start = sel_start_in_line.max(row.start_offset);
                            let row_sel_end = sel_end_in_line.min(row.end_offset);

                            if row_sel_start < row_sel_end {
                                // Convert to display column (relative to row start)
                                let display_start = row_sel_start - row.start_offset;
                                let display_end = row_sel_end - row.start_offset;
                                selection_rects.push((cursor_idx, row_idx, display_start, display_end, row.is_continuation));
                            }
                        }
                    } else {
                        // Convert buffer line to display row
                        let display_row = fold_state.actual_to_display_line(line_idx);
                        selection_rects.push((cursor_idx, display_row, sel_start_in_line, sel_end_in_line, false));
                    }
                }
            }
        }
    }

    // Also handle backward-compatible selection_start/selection_end if cursors is empty/mismatched
    if state.cursors.is_empty() || (state.cursors.len() == 1 && state.selection_start.is_some()) {
        if let (Some(sel_start), Some(sel_end)) = (state.selection_start, state.selection_end) {
            let (start, end) = if sel_start <= sel_end {
                (sel_start, sel_end)
            } else {
                (sel_end, sel_start)
            };

            if start != end && selection_rects.is_empty() {
                let start_line = state.rope.char_to_line(start);
                let end_line = state.rope.char_to_line(end);

                for line_idx in start_line..=end_line {
                    // Skip hidden lines
                    if fold_state.is_line_hidden(line_idx) {
                        continue;
                    }

                    let line_start_char = state.rope.line_to_char(line_idx);
                    let line = state.rope.line(line_idx);

                    let sel_start_in_line = if line_idx == start_line {
                        start - line_start_char
                    } else {
                        0
                    };

                    let sel_end_in_line = if line_idx == end_line {
                        end - line_start_char
                    } else {
                        line.len_chars()
                    };

                    if sel_start_in_line < sel_end_in_line {
                        if use_wrapping {
                            for (row_idx, row) in state.display_map.rows.iter().enumerate() {
                                if row.buffer_line != line_idx {
                                    continue;
                                }
                                let row_sel_start = sel_start_in_line.max(row.start_offset);
                                let row_sel_end = sel_end_in_line.min(row.end_offset);

                                if row_sel_start < row_sel_end {
                                    let display_start = row_sel_start - row.start_offset;
                                    let display_end = row_sel_end - row.start_offset;
                                    selection_rects.push((0, row_idx, display_start, display_end, row.is_continuation));
                                }
                            }
                        } else {
                            // Convert buffer line to display row
                            let display_row = fold_state.actual_to_display_line(line_idx);
                            selection_rects.push((0, display_row, sel_start_in_line, sel_end_in_line, false));
                        }
                    }
                }
            }
        }
    }

    // Clear all if no selections
    if selection_rects.is_empty() {
        for (_, _, _, mut visibility, _) in selection_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    let mut existing_selections: Vec<_> = selection_query.iter_mut().collect();
    let mut entity_index = 0;

    for (cursor_idx, row_idx, sel_start_col, sel_end_col, is_continuation) in selection_rects {
        let selection_width = (sel_end_col - sel_start_col) as f32 * char_width;

        // Add continuation indent for wrapped lines
        let extra_indent = if use_wrapping && is_continuation && settings.wrapping.indent_wrapped_lines {
            settings.indentation.indent_size as f32 * char_width
        } else {
            0.0
        };

        let x_left_edge = settings.ui.layout.code_margin_left + extra_indent + (sel_start_col as f32 * char_width);
        let y_from_top = settings.ui.layout.margin_top + state.scroll_offset + (row_idx as f32 * line_height);

        let sprite_center_x =
            -(viewport.width as f32) / 2.0 + x_left_edge + selection_width / 2.0;
        let sprite_center_y = (viewport.height as f32) / 2.0 - y_from_top;

        let translation = Vec3::new(sprite_center_x, sprite_center_y, 0.5);

        if entity_index < existing_selections.len() {
            let (_, ref mut transform, ref mut sprite, ref mut visibility, ref mut marker) =
                &mut existing_selections[entity_index];
            sprite.custom_size = Some(Vec2::new(selection_width, line_height));
            transform.translation = translation;
            marker.line_index = row_idx;
            marker.cursor_index = cursor_idx;
            **visibility = Visibility::Visible;
        } else {
            commands.spawn((
                Sprite {
                    color: settings.theme.selection_background,
                    custom_size: Some(Vec2::new(selection_width, line_height)),
                    ..default()
                },
                Transform::from_translation(translation),
                SelectionHighlight { line_index: row_idx, cursor_index: cursor_idx },
                Name::new(format!("Selection_C{}_R{}", cursor_idx, row_idx)),
                Visibility::Visible,
            ));
        }

        entity_index += 1;
    }

    // Hide unused selections
    for i in entity_index..existing_selections.len() {
        let (_, _, _, ref mut visibility, _) = &mut existing_selections[i];
        **visibility = Visibility::Hidden;
    }
}

/// Update indent guide rendering
pub(crate) fn update_indent_guides(
    mut commands: Commands,
    state: Res<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
    fold_state: Res<FoldState>,
    mut guide_query: Query<(Entity, &mut Transform, &mut Visibility, &mut IndentGuide)>,
) {
    // Hide all guides if disabled
    if !settings.ui.show_indent_guides {
        for (_, _, mut visibility, _) in guide_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    if !state.is_changed() {
        return;
    }

    let line_height = settings.font.line_height;
    let char_width = settings.font.char_width;
    let indent_size = settings.indentation.indent_size;
    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;

    // Calculate visible display row range
    let visible_start_row = ((-state.scroll_offset) / line_height).floor() as usize;
    let visible_lines = ((viewport_height / line_height).ceil() as usize) + 2;
    let visible_end_row = visible_start_row + visible_lines;

    // Collect guides needed for visible lines
    // Each guide is identified by (display_row, indent_level)
    let mut needed_guides: Vec<(usize, usize)> = Vec::new();

    // === OPTIMIZATION: Start from approximate visible row instead of row 0 ===
    // For files with no folding, we can jump directly to the visible start
    // This changes O(all_lines) to O(visible_lines)
    let total_lines = state.rope.len_lines();
    let has_folding = !fold_state.regions.is_empty();

    // Calculate starting buffer line
    let start_buffer_line = if has_folding {
        // With folding, we need to iterate to find the right buffer line
        // But we can still skip most lines quickly
        let mut display_row = 0;
        let mut buffer_line = 0;
        while buffer_line < total_lines && display_row < visible_start_row {
            if !fold_state.is_line_hidden(buffer_line) {
                display_row += 1;
            }
            buffer_line += 1;
        }
        buffer_line
    } else {
        // No folding: display_row == buffer_line, jump directly
        visible_start_row.min(total_lines)
    };

    // Start display row at visible_start_row (or the actual row if we started earlier)
    let mut current_display_row: usize = if has_folding {
        // With folding, we tracked this while finding start_buffer_line
        let mut display_row = 0;
        for bl in 0..start_buffer_line {
            if !fold_state.is_line_hidden(bl) {
                display_row += 1;
            }
        }
        display_row
    } else {
        start_buffer_line
    };

    // Iterate only through visible buffer lines
    for buffer_line in start_buffer_line..total_lines {
        // Skip hidden lines
        if fold_state.is_line_hidden(buffer_line) {
            continue;
        }

        // Stop if past visible range
        if current_display_row > visible_end_row {
            break;
        }

        let line = state.rope.line(buffer_line);

        // Count leading whitespace to determine indentation
        let mut leading_spaces = 0;
        for c in line.chars() {
            match c {
                ' ' => leading_spaces += 1,
                '\t' => leading_spaces += indent_size,
                _ => break,
            }
        }

        // Calculate number of indent levels
        let indent_levels = leading_spaces / indent_size;

        // Add a guide for each indent level (using display_row for position)
        for level in 0..indent_levels {
            needed_guides.push((current_display_row, level));
        }

        current_display_row += 1;
    }

    // Collect existing guide entities
    let mut existing_guides: Vec<_> = guide_query.iter_mut().collect();
    let mut entity_index = 0;

    for (display_row, level) in needed_guides.iter() {
        let x_offset = settings.ui.layout.code_margin_left + (*level * indent_size) as f32 * char_width;
        let y_offset = settings.ui.layout.margin_top + state.scroll_offset + (*display_row as f32 * line_height);

        // Position the guide line (thin vertical line)
        let sprite_x = -viewport_width / 2.0 + x_offset - state.horizontal_scroll_offset + viewport.offset_x;
        let sprite_y = viewport_height / 2.0 - y_offset;
        let translation = Vec3::new(sprite_x, sprite_y, 0.1); // z=0.1 behind text

        if entity_index < existing_guides.len() {
            // Reuse existing entity
            let (_, ref mut transform, ref mut visibility, ref mut guide) = &mut existing_guides[entity_index];
            transform.translation = translation;
            guide.level = *level;
            guide.line_index = *display_row;
            **visibility = Visibility::Visible;
        } else {
            // Spawn new guide entity
            commands.spawn((
                Sprite {
                    color: settings.theme.indent_guide,
                    custom_size: Some(Vec2::new(1.0, line_height)),
                    ..default()
                },
                Transform::from_translation(translation),
                IndentGuide {
                    level: *level,
                    line_index: *display_row,
                },
                Name::new(format!("IndentGuide_{}_{}", display_row, level)),
                Visibility::Visible,
            ));
        }

        entity_index += 1;
    }

    // Hide unused guide entities
    for i in entity_index..existing_guides.len() {
        let (_, _, ref mut visibility, _) = &mut existing_guides[i];
        **visibility = Visibility::Hidden;
    }
}

/// Animate smooth scrolling by interpolating towards target scroll offset
pub(crate) fn animate_smooth_scroll(
    mut state: ResMut<CodeEditorState>,
    time: Res<Time>,
    settings: Res<EditorSettings>,
) {
    if !settings.scrolling.smooth_scrolling {
        // When smooth scrolling is disabled, sync target with actual
        state.target_scroll_offset = state.scroll_offset;
        state.target_horizontal_scroll_offset = state.horizontal_scroll_offset;
        return;
    }

    // Smooth scrolling interpolation factor (higher = faster)
    // Using exponential decay for natural feel
    let smoothness = 12.0; // Adjust for desired smoothness
    let dt = time.delta_secs();
    let t = 1.0 - (-smoothness * dt).exp();

    // Vertical scroll animation
    let vertical_diff = state.target_scroll_offset - state.scroll_offset;
    if vertical_diff.abs() > 0.1 {
        state.scroll_offset += vertical_diff * t;
        state.needs_scroll_update = true;
    } else if vertical_diff.abs() > 0.0 {
        // Snap to target when close enough
        state.scroll_offset = state.target_scroll_offset;
        state.needs_scroll_update = true;
    }

    // Horizontal scroll animation
    let horizontal_diff = state.target_horizontal_scroll_offset - state.horizontal_scroll_offset;
    if horizontal_diff.abs() > 0.1 {
        state.horizontal_scroll_offset += horizontal_diff * t;
        state.needs_update = true; // Horizontal scroll needs full update
    } else if horizontal_diff.abs() > 0.0 {
        // Snap to target when close enough
        state.horizontal_scroll_offset = state.target_horizontal_scroll_offset;
        state.needs_update = true;
    }
}

/// Auto-scroll viewport to keep cursor visible
pub(crate) fn auto_scroll_to_cursor(
    mut state: ResMut<CodeEditorState>,
    settings: Res<EditorSettings>,
    viewport: Res<ViewportDimensions>,
) {
    // Only auto-scroll when cursor actually moves (not when scroll changes)
    let cursor_pos = state.cursor_pos.min(state.rope.len_chars());
    if cursor_pos == state.last_cursor_pos {
        return;
    }

    // Update last cursor position
    state.last_cursor_pos = cursor_pos;
    let line_index = state.rope.char_to_line(cursor_pos);
    let line_height = settings.font.line_height;
    let viewport_height = viewport.height as f32;
    let viewport_width = viewport.width as f32;

    // === VERTICAL AUTO-SCROLL ===

    // Calculate cursor's Y position
    let cursor_y = settings.ui.layout.margin_top + state.scroll_offset + (line_index as f32 * line_height);

    // Define visible range (with some margin)
    let margin_vertical = line_height * 2.0;
    let visible_top = margin_vertical;
    let visible_bottom = viewport_height - margin_vertical;

    // Adjust scroll if cursor is outside visible range
    if cursor_y < visible_top {
        // Cursor is above visible area - scroll up
        state.scroll_offset += visible_top - cursor_y;
        state.needs_scroll_update = true;
    } else if cursor_y > visible_bottom {
        // Cursor is below visible area - scroll down
        state.scroll_offset -= cursor_y - visible_bottom;
        state.needs_scroll_update = true;
    }

    // Clamp scroll_offset to valid range
    state.scroll_offset = state.scroll_offset.min(0.0);
    let line_count = state.rope.len_lines();
    let content_height = line_count as f32 * line_height;
    let max_scroll = -(content_height - viewport_height + settings.ui.layout.margin_top);
    state.scroll_offset = state.scroll_offset.max(max_scroll.min(0.0));

    // === HORIZONTAL AUTO-SCROLL ===

    // Calculate cursor's X position (column within line)
    let line_start = state.rope.line_to_char(line_index);
    let col_index = cursor_pos - line_start;
    let char_width = settings.font.char_width;

    // Cursor X position relative to code area (before scrolling)
    let cursor_x = col_index as f32 * char_width;

    // Define horizontal visible range (with some margin)
    let margin_horizontal = char_width * 5.0; // 5 characters of margin
    let visible_left = state.horizontal_scroll_offset;
    let visible_right = state.horizontal_scroll_offset + viewport_width - settings.ui.layout.code_margin_left - margin_horizontal;

    // Adjust horizontal scroll if cursor is outside visible range
    if cursor_x < visible_left {
        // Cursor is left of visible area - scroll left
        state.horizontal_scroll_offset = cursor_x.max(0.0);
        state.needs_scroll_update = true;
    } else if cursor_x > visible_right {
        // Cursor is right of visible area - scroll right
        state.horizontal_scroll_offset = cursor_x - (viewport_width - settings.ui.layout.code_margin_left - margin_horizontal);
        state.needs_scroll_update = true;
    }

    // Clamp horizontal_scroll_offset to valid range
    // Minimum is 0.0 (don't scroll past the left edge)
    state.horizontal_scroll_offset = state.horizontal_scroll_offset.max(0.0);

    // Maximum is when rightmost content reaches viewport edge
    let max_horizontal_scroll = (state.max_content_width - viewport_width).max(0.0);
    state.horizontal_scroll_offset = state.horizontal_scroll_offset.min(max_horizontal_scroll);
}
