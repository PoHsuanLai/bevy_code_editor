//! UI elements: selection, indent guides

use super::editor_ui_plugin::EditorRenderConfig;
use crate::settings::*;
use crate::text_view::{
    DisplayLayout, RectOverlay, RowVertical, TextViewOverlays, TextViewState, TextViewViewport,
};
use crate::types::*;
use bevy::prelude::*;
use bevy_text_engine::FontConfig;
use bevy_text_editor::ScrollConfig;

/// Push selection rectangles into `TextViewOverlays` for all cursors.
///
/// Selections render as paint-time overlay rects with `z = -1` (below text),
/// not as separate `Sprite` entities, so the engine's renderer paints them
/// in the same draw call as the glyphs.
pub(crate) fn update_selection_highlight(
    mut editor_query: Query<
        (
            &TextViewState,
            &TextViewViewport,
            &SelectionState,
            &mut TextViewOverlays,
            &FoldState,
            &FontConfig,
            Option<&DisplayLayout>,
            &ThemeConfig,
        ),
        With<CodeEditor>,
    >,
) {
    for (tv, _vp, sel, mut overlays, fold_state, font, layout, theme) in
        editor_query.iter_mut()
    {
    // Drain any selection rects from the previous frame (z = -1 marks selection;
    // cursor caret uses z = +1; z = 0 is reserved for line-bg/highlight overlays).
    overlays.rects.retain(|r| r.z != -1);

    let char_width = font.char_width;
    let _ = font.line_height;

    // Collect (start_char, end_char) for every active selection range. The
    // SelectionCollection is the single source of truth — emit one rect-set
    // per non-empty selection.
    let selections: Vec<(usize, usize)> = sel
        .selections
        .iter()
        .filter(|s| s.has_selection())
        .map(|s| s.range())
        .collect();

    for (start, end) in selections {
        let start_line = tv.rope.char_to_line(start);
        let end_line = tv.rope.char_to_line(end);

        for line_idx in start_line..=end_line {
            if fold_state.is_line_hidden(line_idx) {
                continue;
            }

            let line_start_char = tv.rope.line_to_char(line_idx);
            let line = tv.rope.line(line_idx);
            let line_chars = line.len_chars();

            let sel_start_col = if line_idx == start_line {
                start - line_start_char
            } else {
                0
            };
            let sel_end_col = if line_idx == end_line {
                end - line_start_char
            } else {
                line_chars
            };
            if sel_start_col >= sel_end_col {
                continue;
            }

            let s_byte = line.slice(..sel_start_col.min(line_chars)).len_bytes();
            let e_byte = line.slice(..sel_end_col.min(line_chars)).len_bytes();

            push_selection_for_buffer_range(
                SelSpan {
                    s_byte,
                    e_byte,
                    sel_start_col,
                    sel_end_col,
                    is_last_buffer_line: line_idx == end_line,
                },
                &RowMap { layout, fold_state, line_idx },
                char_width,
                theme.selection_background,
                &mut overlays.rects,
            );
        }
    }

    overlays.version = overlays.version.wrapping_add(1);
    }
}

/// One buffer line's slice of a selection: the byte range, the matching
/// char range (used as a fallback when shaping is unavailable), and a flag
/// marking whether this line is the *last* line of the multi-line selection
/// (the only line that must end at the actual end-x rather than extending to
/// the row's right edge).
struct SelSpan {
    s_byte: usize,
    e_byte: usize,
    sel_start_col: usize,
    sel_end_col: usize,
    is_last_buffer_line: bool,
}

/// Read-only context for mapping `(buffer_row, byte)` → `(display_row, byte_in_row)`.
/// `layout` is `None` for off-viewport buffer lines, in which case the row
/// maps via `fold_state` and pixel math falls back to `char_width`.
struct RowMap<'a> {
    layout: Option<&'a DisplayLayout>,
    fold_state: &'a FoldState,
    /// Buffer line index, in `usize` for `fold_state` lookups. Equals the
    /// `buffer_row` passed to `layout.buffer_to_display` (which takes `u32`).
    line_idx: usize,
}

impl<'a> RowMap<'a> {
    fn buffer_row(&self) -> u32 {
        self.line_idx as u32
    }

    /// Resolve `(display_row, byte_in_row)` for a byte offset within the
    /// buffer line. Fall back to fold-state's display row + raw byte when
    /// the layout doesn't cover this row.
    fn locate(&self, byte_in_line: usize) -> (u32, usize) {
        self.layout
            .and_then(|l| l.buffer_to_display(self.buffer_row(), byte_in_line))
            .unwrap_or_else(|| {
                (
                    self.fold_state.actual_to_display_line(self.line_idx) as u32,
                    byte_in_line,
                )
            })
    }

    /// Pixel x where the given display row's text ends (line-local).
    fn row_text_end_x(&self, display_row: u32, char_width: f32) -> f32 {
        self.layout
            .and_then(|l| {
                l.lines
                    .iter()
                    .find(|line| line.display_row == display_row)
                    .and_then(|line| l.x_at_byte(display_row, line.text.len()))
            })
            .unwrap_or(char_width)
    }
}

/// Push selection rects for one buffer line's slice. With wrap on, the slice
/// may span multiple display rows; emit one rect per row, extending non-final
/// rows to the row's right edge so the selection band looks continuous.
fn push_selection_for_buffer_range(
    span: SelSpan,
    rows: &RowMap<'_>,
    char_width: f32,
    color: Color,
    out: &mut Vec<RectOverlay>,
) {
    let (start_row, start_byte_in_row) = rows.locate(span.s_byte);
    let (end_row, end_byte_in_row) = rows.locate(span.e_byte);

    let start_x = rows
        .layout
        .and_then(|l| l.x_at_byte(start_row, start_byte_in_row))
        .unwrap_or(span.sel_start_col as f32 * char_width);
    let end_x_resolved = rows
        .layout
        .and_then(|l| l.x_at_byte(end_row, end_byte_in_row))
        .unwrap_or(span.sel_end_col as f32 * char_width);
    // Non-final-line rows extend to the row's text-end so the selection
    // hugs the actual text instead of filling the row to the viewport edge.
    let trailing_x = if span.is_last_buffer_line {
        end_x_resolved
    } else {
        rows.row_text_end_x(end_row, char_width)
    };

    if start_row == end_row {
        out.push(selection_rect(start_row, start_x..trailing_x, color));
        return;
    }

    // Multi-row span (selection crossed a soft-wrap break).
    let start_row_end = rows.row_text_end_x(start_row, char_width).max(start_x + char_width);
    out.push(selection_rect(start_row, start_x..start_row_end, color));
    for r in (start_row + 1)..end_row {
        let r_end = rows.row_text_end_x(r, char_width).max(char_width);
        out.push(selection_rect(r, 0.0..r_end, color));
    }
    out.push(selection_rect(end_row, 0.0..trailing_x, color));
}

fn selection_rect(display_row: u32, x_range: std::ops::Range<f32>, color: Color) -> RectOverlay {
    RectOverlay {
        display_row,
        x_range,
        vertical: RowVertical::Full,
        color,
        z: -1,
        corners: bevy_text_engine::CornerRadii::ZERO,
    }
}

/// Update indent guide rendering
pub(crate) fn update_indent_guides(
    mut commands: Commands,
    editor_query: Query<
        (&TextViewState, &TextViewViewport, &FoldState, &FontConfig, &ThemeConfig),
        With<CodeEditor>,
    >,
    ui: Res<UiSettings>,
    indentation: Res<IndentationSettings>,
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

    let indent_size = indentation.indent_size;

    // Collect existing guide entities once (shared pool across editors)
    let mut existing_guides: Vec<_> = guide_query.iter_mut().collect();
    let mut entity_index = 0;

    for (tv, vp, fold_state, font, theme) in editor_query.iter() {
        let line_height = font.line_height;
        let char_width = font.char_width;
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
    }

    // Hide unused guide entities
    for i in entity_index..existing_guides.len() {
        let (_, _, ref mut visibility, _) = &mut existing_guides[i];
        **visibility = Visibility::Hidden;
    }
}

/// Animate smooth scrolling by interpolating towards target scroll offset
pub(crate) fn animate_smooth_scroll(
    mut editor_query: Query<
        (
            &mut TextViewState,
            &super::scrollbar::ScrollbarDragState,
            &ScrollConfig,
        ),
        With<CodeEditor>,
    >,
    time: Res<Time>,
) {
    for (mut tv, scrollbar_drag, scroll_cfg) in editor_query.iter_mut() {
        // When dragging scrollbar or smooth scrolling disabled, apply target immediately
        let is_dragging = scrollbar_drag.is_dragging;

        let use_smooth = scroll_cfg.smooth && !is_dragging;

        if !use_smooth {
            // Instant update - no interpolation
            if (tv.target_scroll_offset - tv.scroll_offset).abs() > 0.001 {
                tv.scroll_offset = tv.target_scroll_offset;
            }
            if (tv.target_horizontal_scroll_offset - tv.horizontal_scroll_offset).abs() > 0.001 {
                tv.horizontal_scroll_offset = tv.target_horizontal_scroll_offset;
            }
            continue;
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
        } else if vertical_diff.abs() > 0.0 {
            // Snap to target when close enough
            tv.scroll_offset = tv.target_scroll_offset;
        }

        // Horizontal scroll animation
        let horizontal_diff = tv.target_horizontal_scroll_offset - tv.horizontal_scroll_offset;
        if horizontal_diff.abs() > 0.1 {
            tv.horizontal_scroll_offset += horizontal_diff * t;
        } else if horizontal_diff.abs() > 0.0 {
            // Snap to target when close enough
            tv.horizontal_scroll_offset = tv.target_horizontal_scroll_offset;
        }
    }
}

/// Run condition: auto-scroll only fires for editors that have moved their
/// cursor and aren't currently being mouse-dragged or scrollbar-dragged.
///
/// Drag suppression is per-entity (Component) — dragging in editor A no
/// longer blocks auto-scroll in editor B (the previous global Resource shape).
pub(crate) fn should_auto_scroll(
    editor_query: Query<
        (
            &TextViewState,
            &CursorState,
            &super::scrollbar::ScrollbarDragState,
            &bevy_text_editor::TextViewDragState,
        ),
        With<CodeEditor>,
    >,
) -> bool {
    for (tv, cursor, scrollbar_drag, mouse_drag) in editor_query.iter() {
        if scrollbar_drag.is_dragging || mouse_drag.is_dragging {
            continue;
        }
        let cursor_pos = cursor.cursor_pos.min(tv.rope.len_chars());
        if cursor_pos != cursor.last_cursor_pos {
            return true;
        }
    }
    false
}

/// Auto-scroll viewport to keep cursor visible
/// Writes to target_scroll_offset, not scroll_offset (applied by animate_smooth_scroll)
pub(crate) fn auto_scroll_to_cursor(
    mut editor_query: Query<
        (
            &mut TextViewState,
            &mut CursorState,
            &TextViewViewport,
            &FontConfig,
        ),
        With<CodeEditor>,
    >,
) {
    for (mut tv, mut cursor, vp, font) in editor_query.iter_mut() {
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
            continue;
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
}
