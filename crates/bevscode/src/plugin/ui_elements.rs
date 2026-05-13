//! UI elements: selection, indent guides

use crate::settings::*;
use bevy_instanced_text_edit::RopeBuffer;
use crate::text_view::{
    DisplayLayout, RectOverlay, RowVertical, ScrollState, TextBuffer, TextViewOverlays,
};
use crate::types::*;
use bevy::prelude::*;
use bevy::ui::ComputedNode;
use bevy_instanced_text::{visible_buffer_range, HiddenLines, MonoCellWidth, TextBounds};

type AutoScrollQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static TextBuffer<RopeBuffer>,
        &'static mut ScrollState,
        &'static crate::text_view::ContentMetrics,
        &'static mut CursorState,
        &'static ComputedNode,
        &'static TextFont,
        &'static bevy::text::LineHeight,
        &'static MonoCellWidth,
    ),
    With<CodeEditor>,
>;

/// Push selection rectangles into `TextViewOverlays` for all cursors.
///
/// Selections render as paint-time overlay rects with `z = -1` (below text),
/// not as separate `Sprite` entities, so the engine's renderer paints them
/// in the same draw call as the glyphs.
///
/// Visible-window clipped: a selection covering the entire 150k-line buffer
/// only emits rects for the ~50 lines actually on screen. The selection
/// itself still spans the whole buffer (kept by `SelectionState`); we just
/// don't paint rects we can't see. Without this clip, Cmd+A on a big file
/// allocates 150k `RectOverlay`s every frame and hangs the editor for
/// seconds.
///
/// Change-detection gated: idle frames do nothing.
#[allow(clippy::type_complexity)]
pub(crate) fn update_selection_highlight(
    mut editor_query: Query<
        (
            Entity,
            &TextBuffer<RopeBuffer>,
            &ComputedNode,
            &ScrollState,
            &SelectionState,
            &mut TextViewOverlays,
            &FoldState,
            &TextFont,
            &bevy::text::LineHeight,
            &MonoCellWidth,
            Option<&DisplayLayout>,
            Option<&HiddenLines>,
            Option<&TextBounds>,
            &EditorTheme,
        ),
        With<CodeEditor>,
    >,
    dirty_editors: Query<
        Entity,
        (
            With<CodeEditor>,
            Or<(
                Changed<SelectionState>,
                Changed<ScrollState>,
                Changed<ComputedNode>,
                Changed<TextBuffer<RopeBuffer>>,
                Changed<FoldState>,
                Changed<MonoCellWidth>,
                Changed<EditorTheme>,
            )>,
        ),
    >,
) {
    let dirty: std::collections::HashSet<Entity> = dirty_editors.iter().collect();
    if dirty.is_empty() {
        return;
    }

    for (
        editor_entity,
        buffer,
        computed,
        scroll,
        sel,
        mut overlays,
        fold_state,
        font,
        lh,
        mono,
        layout,
        hidden,
        wrap,
        theme,
    ) in editor_query.iter_mut()
    {
        if !dirty.contains(&editor_entity) {
            continue;
        }
        // Drain any selection rects from the previous frame (z = -1 marks selection;
        // cursor caret uses z = +1; z = 0 is reserved for line-bg/highlight overlays).
        overlays.rects.retain(|r| r.z != -1);

        let char_width = mono.px;
        let line_height = bevy_instanced_text::resolve_line_height(*lh, font.font_size);

        // Visible buffer-line window. Selections are clipped to this band so
        // a multi-thousand-line selection doesn't allocate per-line rects for
        // off-viewport rows.
        let inv = computed.inverse_scale_factor();
        let viewport_height = computed.size().y * inv;
        let text_area_top = computed.content_inset().min_inset.y * inv;
        let wrap_cfg = wrap.copied().unwrap_or_default();
        let visible = visible_buffer_range(&**buffer, scroll, viewport_height, text_area_top, line_height, char_width, wrap_cfg, hidden);
        if visible.start >= visible.end {
            overlays.version = overlays.version.wrapping_add(1);
            continue;
        }

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
            let sel_start_line = buffer.char_to_line(start);
            let sel_end_line = buffer.char_to_line(end);

            // Iterate only the part of the selection that overlaps the
            // visible window. Off-viewport portions still exist in
            // `SelectionState` — we just don't emit rects for them.
            let iter_start = sel_start_line.max(visible.start);
            let iter_end = sel_end_line.min(visible.end.saturating_sub(1));
            if iter_start > iter_end {
                continue;
            }

            for line_idx in iter_start..=iter_end {
                if fold_state.is_line_hidden(line_idx) {
                    continue;
                }

                let line_start_char = buffer.line_to_char(line_idx);
                let line = buffer.line(line_idx);
                let line_chars = line.len_chars();

                let sel_start_col = if line_idx == sel_start_line {
                    start - line_start_char
                } else {
                    0
                };
                let sel_end_col = if line_idx == sel_end_line {
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
                        is_last_buffer_line: line_idx == sel_end_line,
                    },
                    &RowMap {
                        layout,
                        fold_state,
                        line_idx,
                    },
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
}

/// Push selection rects for one buffer line's slice. With wrap on, the slice
/// may span multiple display rows; emit one rect per row, extending non-final
/// rows to the row's right edge so the selection band looks continuous.
///
/// Width-fallback note: when a row isn't in `layout.lines` (i.e. shaped) we
/// fall back to `sel_end_col * char_width`. For lines that *are* in the
/// visible buffer-line range but *outside* the layout's narrower shaped
/// slice — which happens at the visible-window edges and during scroll —
/// the caller has the actual selection extent in chars and that's a much
/// better width than a single `char_width`. Without this, boundary lines
/// get a single-char-wide selection rect while shaped lines get the full
/// line rect.
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
    // Width fallback for unshaped rows: use the selection extent in chars.
    let end_chars_fallback = span.sel_end_col as f32 * char_width;
    let row_end_or_chars = |row: u32| -> f32 {
        rows.layout
            .and_then(|l| {
                l.lines
                    .iter()
                    .find(|line| line.display_row == row)
                    .and_then(|line| l.x_at_byte(row, line.text.len()))
            })
            .unwrap_or(end_chars_fallback)
    };
    // Non-final-line rows extend to the row's text-end so the selection
    // hugs the actual text instead of filling the row to the viewport edge.
    let trailing_x = if span.is_last_buffer_line {
        end_x_resolved
    } else {
        row_end_or_chars(end_row)
    };

    if start_row == end_row {
        out.push(selection_rect(start_row, start_x..trailing_x, color));
        return;
    }

    // Multi-row span (selection crossed a soft-wrap break).
    let start_row_end = row_end_or_chars(start_row).max(start_x + char_width);
    out.push(selection_rect(start_row, start_x..start_row_end, color));
    for r in (start_row + 1)..end_row {
        let r_end = row_end_or_chars(r).max(char_width);
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
        corners: bevy_instanced_text::CornerRadii::ZERO,
    }
}

/// Push a 1-px vertical `RectOverlay` (z = -2) per indent level per
/// visible row, so the engine paints indent guides in the same draw
/// call as the glyphs.
pub(crate) fn update_indent_guides(
    mut editor_query: Query<
        (
            Entity,
            &TextBuffer<RopeBuffer>,
            &ScrollState,
            &ComputedNode,
            &FoldState,
            &TextFont,
            &bevy::text::LineHeight,
            &MonoCellWidth,
            &EditorTheme,
            &mut TextViewOverlays,
            &EditorUi,
            &Indentation,
        ),
        With<CodeEditor>,
    >,
) {
    for (_editor_entity, buffer, scroll, computed, fold_state, font, lh, mono, theme, mut overlays, ui, indentation) in
        editor_query.iter_mut()
    {
        // z lanes: -2 indent guides, -1 selection, +1 caret.
        overlays.rects.retain(|r| r.z != -2);

        if !ui.show_indent_guides {
            overlays.version = overlays.version.wrapping_add(1);
            continue;
        }

        let inv = computed.inverse_scale_factor();
        let indent_size = indentation.indent_size;
        let line_height = bevy_instanced_text::resolve_line_height(*lh, font.font_size);
        let char_width = mono.px;
        let viewport_height = computed.size().y * inv;

        let visible_start_row = ((-scroll.scroll_offset) / line_height).floor().max(0.0) as usize;
        let visible_lines = ((viewport_height / line_height).ceil() as usize) + 2;
        let visible_end_row = visible_start_row + visible_lines;

        let total_lines = buffer.len_lines();
        let has_folding = !fold_state.regions.is_empty();

        // Skip non-visible prefix without scanning every rope line.
        let start_buffer_line = if has_folding {
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
            visible_start_row.min(total_lines)
        };

        let mut current_display_row: usize = if has_folding {
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

        for buffer_line in start_buffer_line..total_lines {
            if fold_state.is_line_hidden(buffer_line) {
                continue;
            }
            if current_display_row > visible_end_row {
                break;
            }

            let line = buffer.line(buffer_line);
            let mut leading_spaces = 0;
            for c in line.chars() {
                match c {
                    ' ' => leading_spaces += 1,
                    '\t' => leading_spaces += indent_size,
                    _ => break,
                }
            }
            let indent_levels = leading_spaces / indent_size;

            for level in 0..indent_levels {
                let x = (level * indent_size) as f32 * char_width;
                overlays.rects.push(RectOverlay {
                    display_row: current_display_row as u32,
                    x_range: x..(x + 1.0),
                    vertical: RowVertical::FullLeaded,
                    color: theme.indent_guide,
                    z: -2,
                    corners: bevy_instanced_text::CornerRadii::ZERO,
                });
            }

            current_display_row += 1;
        }

        overlays.version = overlays.version.wrapping_add(1);
    }
}

/// Run condition: auto-scroll only fires for editors that have moved their
/// cursor and aren't currently being mouse-dragged.
///
/// Drag suppression is per-entity (Component) — dragging in editor A no
/// longer blocks auto-scroll in editor B (the previous global Resource shape).
pub(crate) fn should_auto_scroll(
    editor_query: Query<
        (
            &TextBuffer<RopeBuffer>,
            &CursorState,
            &bevy_instanced_text_edit::TextViewDragState,
        ),
        With<CodeEditor>,
    >,
) -> bool {
    for (buffer, cursor, mouse_drag) in editor_query.iter() {
        if mouse_drag.is_dragging {
            continue;
        }
        let cursor_pos = cursor.cursor_pos.min(buffer.len_chars());
        if cursor_pos != cursor.last_cursor_pos {
            return true;
        }
    }
    false
}

pub(crate) fn auto_scroll_to_cursor(mut editor_query: AutoScrollQuery) {
    for (buffer, mut scroll, metrics, mut cursor, computed, font, lh, mono) in editor_query.iter_mut() {
        // Get cursor position
        let cursor_pos = cursor.cursor_pos.min(buffer.len_chars());

        // Update last cursor position
        cursor.last_cursor_pos = cursor_pos;
        let line_index = buffer.char_to_line(cursor_pos);
        let line_height = bevy_instanced_text::resolve_line_height(*lh, font.font_size);
        let inv = computed.inverse_scale_factor();
        let viewport_height = computed.size().y * inv;
        let viewport_width = computed.size().x * inv;
        let text_area_top = computed.content_inset().min_inset.y * inv;
        let text_area_left = computed.content_inset().min_inset.x * inv;

        // === VERTICAL AUTO-SCROLL ===

        // Calculate cursor's Y position
        let cursor_y = text_area_top + scroll.scroll_offset + (line_index as f32 * line_height);

        // Define visible range (with some margin)
        let margin_vertical = line_height * 2.0;
        let visible_top = margin_vertical;
        let visible_bottom = viewport_height - margin_vertical;

        // Adjust target scroll if cursor is outside visible range
        if cursor_y < visible_top {
            // Cursor is above visible area - scroll up
            scroll.target_scroll_offset += visible_top - cursor_y;
        } else if cursor_y > visible_bottom {
            // Cursor is below visible area - scroll down
            scroll.target_scroll_offset -= cursor_y - visible_bottom;
        } else {
            // Cursor is visible, no auto-scroll needed
            continue;
        }

        // Clamp target_scroll_offset to valid range
        scroll.target_scroll_offset = scroll.target_scroll_offset.min(0.0);
        let line_count = buffer.len_lines();
        let content_height = line_count as f32 * line_height;
        let max_scroll = -(content_height - viewport_height + text_area_top);
        scroll.target_scroll_offset = scroll.target_scroll_offset.max(max_scroll.min(0.0));

        // === HORIZONTAL AUTO-SCROLL ===

        // Calculate cursor's X position (column within line)
        let line_start = buffer.line_to_char(line_index);
        let col_index = cursor_pos - line_start;
        let char_width = mono.px;

        // Cursor X position relative to code area (before scrolling)
        let cursor_x = col_index as f32 * char_width;

        // Define horizontal visible range (with some margin)
        let margin_horizontal = char_width * 5.0; // 5 characters of margin
        let visible_left = scroll.horizontal_scroll_offset;
        let visible_right = scroll.horizontal_scroll_offset + viewport_width
            - text_area_left
            - margin_horizontal;

        // Adjust horizontal target scroll if cursor is outside visible range
        if cursor_x < visible_left {
            // Cursor is left of visible area - scroll left
            scroll.target_horizontal_scroll_offset = cursor_x.max(0.0);
        } else if cursor_x > visible_right {
            // Cursor is right of visible area - scroll right
            scroll.target_horizontal_scroll_offset =
                cursor_x - (viewport_width - text_area_left - margin_horizontal);
        }

        // Clamp target_horizontal_scroll_offset to valid range
        // Minimum is 0.0 (don't scroll past the left edge)
        scroll.target_horizontal_scroll_offset = scroll.target_horizontal_scroll_offset.max(0.0);

        // Maximum is when rightmost content reaches viewport edge
        let max_horizontal_scroll = (metrics.max_content_width - viewport_width).max(0.0);
        scroll.target_horizontal_scroll_offset = scroll
            .target_horizontal_scroll_offset
            .min(max_horizontal_scroll);
    }
}
