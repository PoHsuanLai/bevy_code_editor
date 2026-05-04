//! UI elements: selection, indent guides

use super::editor_ui_plugin::EditorRenderConfig;
use crate::settings::*;
use crate::text_view::{
    RectOverlay, RowVertical, TextViewOverlays, TextViewState, TextViewViewport,
};
use crate::types::*;
use bevy::prelude::*;

/// Push selection rectangles into `TextViewOverlays` for all cursors.
///
/// As of step 6b, selections are paint-time overlays (z = -1, below text)
/// rather than Sprite entities. The collection logic (which lines/cols are
/// selected) is unchanged; only the writing-out is different.
pub(crate) fn update_selection_highlight(
    editor_query: Query<
        (
            &TextViewState,
            &TextViewViewport,
            &CodeEditorState,
            &SelectionState,
            &EditorDisplayState,
            &CursorState,
            &mut TextViewOverlays,
        ),
        With<CodeEditor>,
    >,
    font: Res<FontSettings>,
    theme: Res<ThemeSettings>,
    wrapping: Res<WrappingSettings>,
    indentation: Res<IndentationSettings>,
    #[cfg(feature = "folding")] fold_state: Res<FoldState>,
) {
    let mut iter = editor_query;
    let Ok((tv, _vp, _editor, sel, display, cursor, mut overlays)) = iter.single_mut() else {
        return;
    };

    // Drain any selection rects from the previous frame (z = -1 marks selection;
    // cursor caret uses z = +1; z = 0 is reserved for line-bg/highlight overlays).
    overlays.rects.retain(|r| r.z != -1);

    let char_width = font.char_width;
    let _ = font.line_height;

    // Check if we're using soft line wrapping
    let use_wrapping = wrapping.enabled && display.display_map.wrap_width > 0;

    // Collect all selection ranges from all cursors
    // (cursor_idx, display_row, start_col, end_col, is_continuation)
    let mut selection_rects: Vec<(usize, usize, usize, usize, bool)> = Vec::new();

    for (cursor_idx, cur) in cursor.cursors.iter().enumerate() {
        if let Some((start, end)) = cur.selection_range() {
            if start == end {
                continue;
            }

            let start_line = tv.rope.char_to_line(start);
            let end_line = tv.rope.char_to_line(end);

            for line_idx in start_line..=end_line {
                // Skip hidden lines
                #[cfg(feature = "folding")]
                if fold_state.is_line_hidden(line_idx) {
                    continue;
                }

                let line_start_char = tv.rope.line_to_char(line_idx);
                let line = tv.rope.line(line_idx);

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
                        for (row_idx, row) in display.display_map.rows.iter().enumerate() {
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
                                selection_rects.push((
                                    cursor_idx,
                                    row_idx,
                                    display_start,
                                    display_end,
                                    row.is_continuation,
                                ));
                            }
                        }
                    } else {
                        // Convert buffer line to display row
                        #[cfg(feature = "folding")]
                        let display_row = fold_state.actual_to_display_line(line_idx);
                        #[cfg(not(feature = "folding"))]
                        let display_row = line_idx;
                        selection_rects.push((
                            cursor_idx,
                            display_row,
                            sel_start_in_line,
                            sel_end_in_line,
                            false,
                        ));
                    }
                }
            }
        }
    }

    // Also handle backward-compatible selection_start/selection_end if cursors is empty/mismatched
    if cursor.cursors.is_empty() || (cursor.cursors.len() == 1 && sel.selection_start.is_some()) {
        if let (Some(sel_start), Some(sel_end)) = (sel.selection_start, sel.selection_end) {
            let (start, end) = if sel_start <= sel_end {
                (sel_start, sel_end)
            } else {
                (sel_end, sel_start)
            };

            if start != end && selection_rects.is_empty() {
                let start_line = tv.rope.char_to_line(start);
                let end_line = tv.rope.char_to_line(end);

                for line_idx in start_line..=end_line {
                    // Skip hidden lines
                    #[cfg(feature = "folding")]
                    if fold_state.is_line_hidden(line_idx) {
                        continue;
                    }

                    let line_start_char = tv.rope.line_to_char(line_idx);
                    let line = tv.rope.line(line_idx);

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
                            for (row_idx, row) in display.display_map.rows.iter().enumerate() {
                                if row.buffer_line != line_idx {
                                    continue;
                                }
                                let row_sel_start = sel_start_in_line.max(row.start_offset);
                                let row_sel_end = sel_end_in_line.min(row.end_offset);

                                if row_sel_start < row_sel_end {
                                    let display_start = row_sel_start - row.start_offset;
                                    let display_end = row_sel_end - row.start_offset;
                                    selection_rects.push((
                                        0,
                                        row_idx,
                                        display_start,
                                        display_end,
                                        row.is_continuation,
                                    ));
                                }
                            }
                        } else {
                            // Convert buffer line to display row
                            #[cfg(feature = "folding")]
                            let display_row = fold_state.actual_to_display_line(line_idx);
                            #[cfg(not(feature = "folding"))]
                            let display_row = line_idx;
                            selection_rects.push((
                                0,
                                display_row,
                                sel_start_in_line,
                                sel_end_in_line,
                                false,
                            ));
                        }
                    }
                }
            }
        }
    }

    // Push selection rects into TextViewOverlays. Done after draining any
    // previous-frame selection rects above; an empty `selection_rects` is
    // equivalent to "no selections this frame".
    for (_cursor_idx, row_idx, sel_start_col, sel_end_col, is_continuation) in selection_rects {
        let extra_indent = if use_wrapping && is_continuation && wrapping.indent_wrapped_lines {
            indentation.indent_size as f32 * char_width
        } else {
            0.0
        };
        let x_left = extra_indent + (sel_start_col as f32 * char_width);
        let x_right = extra_indent + (sel_end_col as f32 * char_width);
        overlays.rects.push(RectOverlay {
            display_row: row_idx as u32,
            x_range: x_left..x_right,
            vertical: RowVertical::Full,
            color: theme.selection_background,
            z: -1, // below text
            corner_radius: 0.0,
        });
    }

    overlays.version = overlays.version.wrapping_add(1);
}

/// Update indent guide rendering
pub(crate) fn update_indent_guides(
    mut commands: Commands,
    editor_query: Query<&TextViewState, With<CodeEditor>>,
    font: Res<FontSettings>,
    theme: Res<ThemeSettings>,
    ui: Res<UiSettings>,
    indentation: Res<IndentationSettings>,
    vp_query: Query<&TextViewViewport, With<CodeEditor>>,
    #[cfg(feature = "folding")] fold_state: Res<FoldState>,
    render_config: Res<EditorRenderConfig>,
    mut guide_query: Query<(Entity, &mut Transform, &mut Visibility, &mut IndentGuide)>,
) {
    // Hide all guides if disabled
    if !ui.show_indent_guides {
        for (_, _, mut visibility, _) in guide_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    let Ok(tv) = editor_query.single() else {
        return;
    };
    let Ok(vp) = vp_query.single() else {
        return;
    };

    // Update when state changes (text edits, scroll, etc.)
    if !tv.needs_update && !tv.needs_scroll_update {
        return;
    }

    let line_height = font.line_height;
    let char_width = font.char_width;
    let indent_size = indentation.indent_size;
    let viewport_height = vp.height as f32;

    // Calculate visible display row range
    let visible_start_row = ((-tv.scroll_offset) / line_height).floor() as usize;
    let visible_lines = ((viewport_height / line_height).ceil() as usize) + 2;
    let visible_end_row = visible_start_row + visible_lines;

    // Collect guides needed for visible lines
    // Each guide is identified by (display_row, indent_level)
    let mut needed_guides: Vec<(usize, usize)> = Vec::new();

    // === OPTIMIZATION: Start from approximate visible row instead of row 0 ===
    // For files with no folding, we can jump directly to the visible start
    // This changes O(all_lines) to O(visible_lines)
    let total_lines = tv.rope.len_lines();
    #[cfg(feature = "folding")]
    let has_folding = !fold_state.regions.is_empty();
    #[cfg(not(feature = "folding"))]
    let has_folding = false;

    // Calculate starting buffer line
    let start_buffer_line = if has_folding {
        #[cfg(feature = "folding")]
        {
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
        }
        #[cfg(not(feature = "folding"))]
        {
            visible_start_row.min(total_lines)
        }
    } else {
        // No folding: display_row == buffer_line, jump directly
        visible_start_row.min(total_lines)
    };

    // Start display row at visible_start_row (or the actual row if we started earlier)
    let mut current_display_row: usize = if has_folding {
        #[cfg(feature = "folding")]
        {
            // With folding, we tracked this while finding start_buffer_line
            let mut display_row = 0;
            for bl in 0..start_buffer_line {
                if !fold_state.is_line_hidden(bl) {
                    display_row += 1;
                }
            }
            display_row
        }
        #[cfg(not(feature = "folding"))]
        {
            start_buffer_line
        }
    } else {
        start_buffer_line
    };

    // Iterate only through visible buffer lines
    for buffer_line in start_buffer_line..total_lines {
        // Skip hidden lines
        #[cfg(feature = "folding")]
        if fold_state.is_line_hidden(buffer_line) {
            continue;
        }

        // Stop if past visible range
        if current_display_row > visible_end_row {
            break;
        }

        let line = tv.rope.line(buffer_line);

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
        let x_offset = vp.text_area_left + (*level * indent_size) as f32 * char_width;
        let y_offset = vp.text_area_top + tv.scroll_offset + (*display_row as f32 * line_height);

        // Position the guide line (thin vertical line)
        // Camera viewport handles panel positioning, so no offset_x here
        let sprite_x = vp.world_left() + x_offset - tv.horizontal_scroll_offset;
        let sprite_y = vp.world_top() - y_offset;
        let translation = Vec3::new(sprite_x, sprite_y, 0.1); // z=0.1 behind text

        if entity_index < existing_guides.len() {
            // Reuse existing entity
            let (_, ref mut transform, ref mut visibility, ref mut guide) =
                &mut existing_guides[entity_index];
            transform.translation = translation;
            guide.level = *level;
            guide.line_index = *display_row;
            **visibility = Visibility::Visible;
        } else {
            // Spawn new guide entity
            let mut entity_cmd = commands.spawn((
                Sprite {
                    color: theme.indent_guide,
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
            if let Some(ref layers) = render_config.render_layers {
                entity_cmd.insert(layers.clone());
            }
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
    mut editor_query: Query<&mut TextViewState, With<CodeEditor>>,
    time: Res<Time>,
    scrolling: Res<ScrollingSettings>,
    _font: Res<crate::settings::FontSettings>,
    _viewport: Res<crate::types::ViewportDimensions>,
    #[cfg(feature = "scrollbar")] scrollbar_drag: Res<super::scrollbar::ScrollbarDragState>,
) {
    let Ok(mut tv) = editor_query.single_mut() else {
        return;
    };

    // When dragging scrollbar or smooth scrolling disabled, apply target immediately
    #[cfg(feature = "scrollbar")]
    let is_dragging = scrollbar_drag.is_dragging;
    #[cfg(not(feature = "scrollbar"))]
    let is_dragging = false;

    let use_smooth = scrolling.smooth && !is_dragging;

    if !use_smooth {
        // Instant update - no interpolation
        if (tv.target_scroll_offset - tv.scroll_offset).abs() > 0.001 {
            tv.scroll_offset = tv.target_scroll_offset;
            tv.needs_scroll_update = true;
        }
        if (tv.target_horizontal_scroll_offset - tv.horizontal_scroll_offset).abs() > 0.001 {
            tv.horizontal_scroll_offset = tv.target_horizontal_scroll_offset;
            tv.needs_update = true;
        }
        return;
    }

    // Smooth scrolling interpolation factor (higher = faster)
    // Using exponential decay for natural feel
    let smoothness = 12.0; // Adjust for desired smoothness
    let dt = time.delta_secs();
    let t = 1.0 - (-smoothness * dt).exp();

    // Vertical scroll animation
    let vertical_diff = tv.target_scroll_offset - tv.scroll_offset;
    if vertical_diff.abs() > 0.1 {
        tv.scroll_offset += vertical_diff * t;
        tv.needs_scroll_update = true;
    } else if vertical_diff.abs() > 0.0 {
        // Snap to target when close enough
        tv.scroll_offset = tv.target_scroll_offset;
        tv.needs_scroll_update = true;
    }

    // Horizontal scroll animation
    let horizontal_diff = tv.target_horizontal_scroll_offset - tv.horizontal_scroll_offset;
    if horizontal_diff.abs() > 0.1 {
        tv.horizontal_scroll_offset += horizontal_diff * t;
        tv.needs_update = true; // Horizontal scroll needs full update
    } else if horizontal_diff.abs() > 0.0 {
        // Snap to target when close enough
        tv.horizontal_scroll_offset = tv.target_horizontal_scroll_offset;
        tv.needs_update = true;
    }
}

/// Run condition: only run auto_scroll_to_cursor when cursor has moved and not dragging scrollbar
pub(crate) fn should_auto_scroll(
    editor_query: Query<(&TextViewState, &CursorState), With<CodeEditor>>,
    #[cfg(feature = "scrollbar")] scrollbar_drag: Res<super::scrollbar::ScrollbarDragState>,
    mouse_drag: Res<crate::input::MouseDragState>,
) -> bool {
    // Don't run when dragging scrollbar
    #[cfg(feature = "scrollbar")]
    if scrollbar_drag.is_dragging {
        return false;
    }

    // Don't run when mouse dragging (causes selection issues due to scroll animation)
    if mouse_drag.is_dragging {
        return false;
    }

    let Ok((tv, cursor)) = editor_query.single() else {
        return false;
    };

    // Only run when cursor has moved
    let cursor_pos = cursor.cursor_pos.min(tv.rope.len_chars());
    cursor_pos != cursor.last_cursor_pos
}

/// Auto-scroll viewport to keep cursor visible
/// Writes to target_scroll_offset, not scroll_offset (applied by animate_smooth_scroll)
pub(crate) fn auto_scroll_to_cursor(
    mut editor_query: Query<
        (&mut TextViewState, &mut CursorState, &TextViewViewport),
        With<CodeEditor>,
    >,
    font: Res<FontSettings>,
) {
    let Ok((mut tv, mut cursor, vp)) = editor_query.single_mut() else {
        return;
    };

    // Get cursor position
    let cursor_pos = cursor.cursor_pos.min(tv.rope.len_chars());

    // Update last cursor position
    cursor.last_cursor_pos = cursor_pos;
    let line_index = tv.rope.char_to_line(cursor_pos);
    let line_height = font.line_height;
    let viewport_height = vp.height as f32;
    let viewport_width = vp.width as f32;

    // === VERTICAL AUTO-SCROLL ===

    // Calculate cursor's Y position
    let cursor_y = vp.text_area_top + tv.scroll_offset + (line_index as f32 * line_height);

    // Define visible range (with some margin)
    let margin_vertical = line_height * 2.0;
    let visible_top = margin_vertical;
    let visible_bottom = viewport_height - margin_vertical;

    // Adjust target scroll if cursor is outside visible range
    if cursor_y < visible_top {
        // Cursor is above visible area - scroll up
        tv.target_scroll_offset += visible_top - cursor_y;
    } else if cursor_y > visible_bottom {
        // Cursor is below visible area - scroll down
        tv.target_scroll_offset -= cursor_y - visible_bottom;
    } else {
        // Cursor is visible, no auto-scroll needed
        return;
    }

    // Clamp target_scroll_offset to valid range
    tv.target_scroll_offset = tv.target_scroll_offset.min(0.0);
    let line_count = tv.rope.len_lines();
    let content_height = line_count as f32 * line_height;
    let max_scroll = -(content_height - viewport_height + vp.text_area_top);
    tv.target_scroll_offset = tv.target_scroll_offset.max(max_scroll.min(0.0));

    // === HORIZONTAL AUTO-SCROLL ===

    // Calculate cursor's X position (column within line)
    let line_start = tv.rope.line_to_char(line_index);
    let col_index = cursor_pos - line_start;
    let char_width = font.char_width;

    // Cursor X position relative to code area (before scrolling)
    let cursor_x = col_index as f32 * char_width;

    // Define horizontal visible range (with some margin)
    let margin_horizontal = char_width * 5.0; // 5 characters of margin
    let visible_left = tv.horizontal_scroll_offset;
    let visible_right =
        tv.horizontal_scroll_offset + viewport_width - vp.text_area_left - margin_horizontal;

    // Adjust horizontal target scroll if cursor is outside visible range
    if cursor_x < visible_left {
        // Cursor is left of visible area - scroll left
        tv.target_horizontal_scroll_offset = cursor_x.max(0.0);
    } else if cursor_x > visible_right {
        // Cursor is right of visible area - scroll right
        tv.target_horizontal_scroll_offset =
            cursor_x - (viewport_width - vp.text_area_left - margin_horizontal);
    }

    // Clamp target_horizontal_scroll_offset to valid range
    // Minimum is 0.0 (don't scroll past the left edge)
    tv.target_horizontal_scroll_offset = tv.target_horizontal_scroll_offset.max(0.0);

    // Maximum is when rightmost content reaches viewport edge
    let max_horizontal_scroll = (tv.max_content_width - viewport_width).max(0.0);
    tv.target_horizontal_scroll_offset = tv
        .target_horizontal_scroll_offset
        .min(max_horizontal_scroll);
}
