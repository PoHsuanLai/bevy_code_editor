//! Shared text-view interactions — scroll, selection, copy.
//!
//! Implemented as observers on `Pointer<…>` events (from `bevy_picking`)
//! and `FocusedInput<KeyboardInput>` (from `bevy_input_focus`), routed by
//! the custom backend in [`crate::picking`]. The polling systems that used
//! to live here (manual cursor-rect hit-testing) are gone — picking +
//! focus dispatch handle entity routing for us.

use bevy::input::keyboard::{KeyCode, KeyboardInput};
use bevy::input_focus::{FocusedInput, InputFocus};
use bevy::input::ButtonState;
use bevy::picking::events::{Drag, Pointer, Press, Release, Scroll};
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use ropey::Rope;

use bevy_text_engine::{
    ContentMetrics, DisplayLayout, FontConfig, ScrollState, TextBuffer, TextView, TextViewViewport,
};

use crate::components::{ScrollConfig, TextViewDragState};
use crate::state::{CursorState, SelectionState};

/// Convert screen coordinates (viewport-local, 0,0 at top-left) to a character
/// position in the rope. Used for click-to-position and drag selection.
///
/// `layout` is consulted when available so proportional fonts hit-test
/// correctly via shaped per-glyph advances; falls back to `font.char_width`
/// column math otherwise.
pub fn screen_to_char_pos(
    screen_pos: Vec2,
    rope: &Rope,
    layout: Option<&DisplayLayout>,
    current_scroll_offset: f32,
    font: &FontConfig,
    viewport: &TextViewViewport,
    scroll_offset_override: Option<f32>,
) -> usize {
    let relative_x = screen_pos.x - viewport.text_area_left;
    let scroll_offset = scroll_offset_override.unwrap_or(current_scroll_offset);
    let relative_y = screen_pos.y - viewport.text_area_top - scroll_offset;

    let line_height = font.line_height;
    let display_row = (relative_y / line_height).max(0.0) as usize;

    let line_count = rope.len_lines();
    if display_row >= line_count {
        return rope.len_chars();
    }

    let line_start_char = rope.line_to_char(display_row);

    if let Some(layout) = layout {
        if let Some(byte_in_row) = layout.byte_at_x(display_row as u32, relative_x) {
            // Use the row's buffer_row + buffer_byte_offset to translate the
            // row-local byte offset to a rope byte. Trivial layouts always
            // have buffer_byte_offset=0 and buffer_row==display_row, so this
            // collapses to the prior behavior; with soft wrap, multiple rows
            // share a buffer line and the offset becomes load-bearing.
            let row = layout
                .lines
                .iter()
                .find(|l| l.display_row == display_row as u32);
            let buffer_line = row.map(|r| r.buffer_row as usize).unwrap_or(display_row);
            let buffer_byte_offset = row.map(|r| r.buffer_byte_offset).unwrap_or(0);
            let line_start_byte = rope.line_to_byte(buffer_line.min(rope.len_lines()));
            let abs_byte =
                (line_start_byte + buffer_byte_offset + byte_in_row).min(rope.len_bytes());
            return rope.byte_to_char(abs_byte);
        }
    }

    let col = (relative_x / font.char_width).max(0.0) as usize;
    let line_len = rope.line(display_row).len_chars().saturating_sub(1);
    let char_in_line = col.min(line_len);
    line_start_char + char_in_line
}

/// Copy the primary selection's text to the system clipboard.
/// Returns true if text was copied, false if no selection.
///
/// Honors the primary selection's [`crate::selection::SelectionMode`]:
/// - `Simple` / `Semantic` — char-range slice (current behavior).
/// - `Block` — column-aligned rectangular slice across visited lines,
///   joined with `\n`. Useful for column edits and reading aligned
///   terminal output.
/// - `Line` — full-line slice (already snapped to whole lines by
///   `expand_to_lines`, so this is identical to the char-range path
///   but kept distinct for clarity).
pub fn copy_selection(sel: &SelectionState, buffer: &TextBuffer) -> bool {
    let Some((start, end)) = sel.primary_range() else {
        return false;
    };
    let mode = sel.selections.primary().mode;
    let len = buffer.rope.len_chars();
    let start = start.min(len);
    let end = end.min(len);
    if start == end {
        return false;
    }
    let text = match mode {
        crate::selection::SelectionMode::Block => block_slice(&buffer.rope, start, end),
        _ => buffer.rope.slice(start..end).to_string(),
    };
    if text.is_empty() {
        return false;
    }
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        let _ = clipboard.set_text(text);
        return true;
    }
    false
}

/// Return the rectangular slice between `start` and `end` (rope char
/// offsets), one row per source line, joined with `\n`. The column
/// range is `[min_col, max_col)` in *characters*, derived from the two
/// endpoints' columns within their lines.
fn block_slice(rope: &Rope, start: usize, end: usize) -> String {
    if start >= end {
        return String::new();
    }
    let (start, end) = (start.min(end), start.max(end));
    let start_line = rope.char_to_line(start);
    let end_line = rope.char_to_line(end);
    let start_col = start - rope.line_to_char(start_line);
    let end_col = end - rope.line_to_char(end_line);
    let (col_lo, col_hi) = if start_col <= end_col {
        (start_col, end_col)
    } else {
        (end_col, start_col)
    };
    if col_lo == col_hi {
        return String::new();
    }
    let mut out = String::new();
    for line_idx in start_line..=end_line {
        let line = rope.line(line_idx);
        let line_len = line.len_chars().saturating_sub(1); // drop trailing '\n'
        let lo = col_lo.min(line_len);
        let hi = col_hi.min(line_len);
        if lo < hi {
            out.push_str(&line.slice(lo..hi).to_string());
        }
        if line_idx != end_line {
            out.push('\n');
        }
    }
    out
}

/// Pointer scroll observer for `TextView` entities — handles both vertical
/// (scroll wheel / two-finger swipe) and horizontal (shift+wheel / two-finger
/// swipe sideways) scrolling.
///
/// Picking already routed this event to the entity under the cursor, so the
/// hit-test loop the old `handle_text_view_scroll` did is gone — we just
/// look up the target entity's components and apply the scroll.
///
/// Horizontal scroll only fires when the view's content width exceeds the
/// available text area (via `ContentMetrics.max_content_width`); the
/// display-map producer maintains that field as it shapes lines.
pub fn on_pointer_scroll(
    trigger: On<Pointer<Scroll>>,
    mut views: Query<
        (
            &TextBuffer,
            &mut ScrollState,
            &ContentMetrics,
            &TextViewViewport,
            &FontConfig,
            Option<&ScrollConfig>,
        ),
        With<TextView>,
    >,
) {
    let entity = trigger.event().entity;
    let Ok((buffer, mut scroll, metrics, viewport, font, scroll_cfg)) = views.get_mut(entity)
    else {
        return;
    };

    let default_scroll = ScrollConfig::default();
    let scroll_cfg = scroll_cfg.unwrap_or(&default_scroll);

    let dx = trigger.event().x;
    let dy = trigger.event().y;

    // Horizontal scroll — only when content overflows.
    if dx.abs() > 0.0 {
        let viewport_width = viewport.width as f32;
        let available_text_width = viewport_width - viewport.text_area_left;
        if metrics.max_content_width > available_text_width {
            let scroll_delta = dx * font.char_width * scroll_cfg.speed;
            let max_h = (metrics.max_content_width - available_text_width).max(0.0);
            if scroll_cfg.smooth {
                scroll.target_horizontal_scroll_offset =
                    (scroll.target_horizontal_scroll_offset + scroll_delta).clamp(0.0, max_h);
            } else {
                scroll.horizontal_scroll_offset =
                    (scroll.horizontal_scroll_offset + scroll_delta).clamp(0.0, max_h);
            }
        }
    }

    // Vertical scroll.
    if dy.abs() > 0.0 {
        let scroll_delta = dy * font.line_height * scroll_cfg.speed;
        let line_count = buffer.rope.len_lines();
        let content_height = line_count as f32 * font.line_height;
        let viewport_height = viewport.height as f32;
        let max_scroll =
            (-(content_height - viewport_height + viewport.text_area_top)).min(0.0);
        if scroll_cfg.smooth {
            scroll.target_scroll_offset =
                (scroll.target_scroll_offset + scroll_delta).clamp(max_scroll, 0.0);
        } else {
            scroll.scroll_offset = (scroll.scroll_offset + scroll_delta).clamp(max_scroll, 0.0);
        }
    }
}

/// Pointer-press observer: focus the view and start a selection drag.
///
/// Only the primary button starts a selection. Position is taken from the
/// hit data, which the picking backend reports in viewport-local coords.
/// Writes through to `SelectionState` (the unified selection model) when
/// present; bare-`TextView` entities without `SelectionState` get focus
/// + drag-tracking but no selection update.
///
/// `CursorState`, when present, is also moved to the click position so
/// editor handlers see the new caret on the next frame.
pub fn on_pointer_press(
    trigger: On<Pointer<Press>>,
    mut views: Query<
        (
            &mut TextViewDragState,
            &TextBuffer,
            &ScrollState,
            &TextViewViewport,
            &FontConfig,
            Option<&DisplayLayout>,
            Option<&mut SelectionState>,
            Option<&mut CursorState>,
        ),
        With<TextView>,
    >,
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut input_focus: ResMut<InputFocus>,
) {
    if trigger.event().button != PointerButton::Primary {
        return;
    }
    let entity = trigger.event().entity;
    let Ok((mut drag_state, buffer, scroll, viewport, font, layout, sel, cursor)) =
        views.get_mut(entity)
    else {
        return;
    };

    let local_pos = match trigger.event().hit.position {
        Some(p) => Vec2::new(p.x, p.y),
        None => return,
    };

    let char_pos = screen_to_char_pos(
        local_pos,
        &buffer.rope,
        layout.as_deref(),
        scroll.scroll_offset,
        font,
        viewport,
        None,
    );

    let alt_held = keyboard.pressed(KeyCode::AltLeft) || keyboard.pressed(KeyCode::AltRight);
    let ctrl_or_cmd_held = keyboard.pressed(KeyCode::ControlLeft)
        || keyboard.pressed(KeyCode::ControlRight)
        || keyboard.pressed(KeyCode::SuperLeft)
        || keyboard.pressed(KeyCode::SuperRight);

    // Ctrl/Cmd-click is a *navigation* gesture (goto-def, etc.) handled by
    // editor observers — skip selection writes here. Alt-click is *not* a
    // navigation gesture in this layer; it switches the upcoming drag into
    // block mode. Editor-side multi-cursor still uses Alt-click but its
    // observer runs separately and reads `SelectionState` directly.
    if ctrl_or_cmd_held {
        input_focus.set(entity);
        return;
    }

    // Click-count detection: same-position click within 0.5s bumps count.
    let now = time.elapsed_secs_f64();
    let near_last = drag_state
        .last_press_pos
        .map(|p| (p - local_pos).length() <= CLICK_RADIUS_PX)
        .unwrap_or(false);
    drag_state.click_count = if near_last && (now - drag_state.last_press_time) <= MULTI_CLICK_SECS
    {
        (drag_state.click_count + 1).min(3)
    } else {
        1
    };
    drag_state.last_press_time = now;
    drag_state.last_press_pos = Some(local_pos);

    let mode = if alt_held {
        crate::selection::SelectionMode::Block
    } else {
        match drag_state.click_count {
            2 => crate::selection::SelectionMode::Semantic,
            3 => crate::selection::SelectionMode::Line,
            _ => crate::selection::SelectionMode::Simple,
        }
    };
    drag_state.mode = mode;

    if let Some(mut sel) = sel {
        match mode {
            crate::selection::SelectionMode::Semantic => {
                let mut s = crate::selection::Selection::cursor(char_pos);
                s.expand_semantic(&buffer.rope, crate::selection::DEFAULT_SEMANTIC_ESCAPE_CHARS);
                sel.selections.clear_secondary();
                *sel.selections.primary_mut() = s;
            }
            crate::selection::SelectionMode::Line => {
                let mut s = crate::selection::Selection::cursor(char_pos);
                s.expand_to_lines(&buffer.rope);
                sel.selections.clear_secondary();
                *sel.selections.primary_mut() = s;
            }
            _ => {
                sel.selections.set_cursor(char_pos);
                sel.selections.primary_mut().mode = mode;
            }
        }
    }
    if let Some(mut cursor) = cursor {
        cursor.cursor_pos = char_pos;
    }
    drag_state.is_dragging = true;
    drag_state.drag_start_pos = Some(char_pos);
    drag_state.drag_start_scroll_offset = scroll.scroll_offset;
    drag_state.last_screen_pos = Some(viewport.hit_test_position + local_pos);
    input_focus.set(entity);
}

/// Two consecutive clicks must fall within this window to count as a
/// multi-click. Matches typical OS double-click thresholds.
const MULTI_CLICK_SECS: f64 = 0.5;
/// Two consecutive clicks must fall within this radius (viewport-local
/// pixels) to count as a multi-click.
const CLICK_RADIUS_PX: f32 = 4.0;

/// Drag observer: extend the selection while the primary button is held.
///
/// Picking dispatches `Pointer<Drag>` to the entity that received the
/// initial press, so this stays scoped to the view that started the drag
/// even if the cursor moves out of its viewport.
///
/// Writes through to `SelectionState` and `CursorState` when present.
pub fn on_pointer_drag(
    trigger: On<Pointer<Drag>>,
    mut views: Query<
        (
            &mut TextViewDragState,
            &TextBuffer,
            &ScrollState,
            &TextViewViewport,
            &FontConfig,
            Option<&DisplayLayout>,
            Option<&mut SelectionState>,
            Option<&mut CursorState>,
        ),
        With<TextView>,
    >,
) {
    if trigger.event().button != PointerButton::Primary {
        return;
    }
    let entity = trigger.event().entity;
    let Ok((mut drag_state, buffer, scroll, viewport, font, layout, sel, cursor)) =
        views.get_mut(entity)
    else {
        return;
    };
    if !drag_state.is_dragging {
        return;
    }

    // Resolve current pointer position in screen space from picking event.
    let cursor_pos = trigger.event().pointer_location.position;

    if let Some(last_pos) = drag_state.last_screen_pos {
        if (cursor_pos - last_pos).length() < 2.0 {
            return;
        }
    }

    let local_pos = cursor_pos - viewport.hit_test_position;
    let char_pos = screen_to_char_pos(
        local_pos,
        &buffer.rope,
        layout.as_deref(),
        scroll.scroll_offset,
        font,
        viewport,
        Some(drag_state.drag_start_scroll_offset),
    );

    if let (Some(mut sel), Some(start)) = (sel, drag_state.drag_start_pos) {
        let mode = drag_state.mode;
        if start == char_pos && mode == crate::selection::SelectionMode::Simple {
            sel.selections.set_cursor(char_pos);
        } else {
            let mut s = crate::selection::Selection::with_mode(char_pos, start, mode);
            match mode {
                crate::selection::SelectionMode::Semantic => {
                    s.expand_semantic(&buffer.rope, crate::selection::DEFAULT_SEMANTIC_ESCAPE_CHARS);
                }
                crate::selection::SelectionMode::Line => {
                    s.expand_to_lines(&buffer.rope);
                }
                _ => {}
            }
            sel.selections.clear_secondary();
            *sel.selections.primary_mut() = s;
        }
    }
    if let Some(mut cursor) = cursor {
        cursor.cursor_pos = char_pos;
    }
    drag_state.last_screen_pos = Some(cursor_pos);
}

/// Release observer: clear the drag flag.
pub fn on_pointer_release(
    trigger: On<Pointer<Release>>,
    mut views: Query<&mut TextViewDragState, With<TextView>>,
) {
    if trigger.event().button != PointerButton::Primary {
        return;
    }
    let entity = trigger.event().entity;
    if let Ok(mut drag_state) = views.get_mut(entity) {
        drag_state.is_dragging = false;
        // mode is *not* reset here — the next press observer rebuilds it
        // from click-count + Alt. Reset to default just in case the next
        // gesture skips press (e.g. focus-only consumers).
        drag_state.mode = crate::selection::SelectionMode::Simple;
    }
}

/// Focused-keyboard observer: copy the selection on Cmd/Ctrl+C.
///
/// Replaces the global `Res<ButtonInput<KeyCode>>` poll with a routed
/// `FocusedInput<KeyboardInput>` event, so only the focused text view's
/// selection is copied. Reads `SelectionState` (the unified selection
/// model); entities without it are skipped.
///
/// Editor entities also have a leafwing-driven `CopyRequested` →
/// `handle_copy` system path. To avoid double-copy, this observer skips
/// when a `TextEditor` Component is present (the editor crate's path
/// handles those). For bare-`TextView + SelectionState` consumers (e.g.
/// a markdown viewer with selectable text), this observer is the only
/// Cmd+C path.
pub fn on_focused_keyboard(
    trigger: On<FocusedInput<KeyboardInput>>,
    views: Query<
        (&SelectionState, &TextBuffer),
        (With<TextView>, Without<crate::state::TextEditor>),
    >,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    let entity = trigger.event().focused_entity;
    let Ok((sel, buffer)) = views.get(entity) else {
        return;
    };

    let event = &trigger.event().input;
    if event.state != ButtonState::Pressed {
        return;
    }
    if event.key_code != KeyCode::KeyC {
        return;
    }
    let ctrl = keyboard.pressed(KeyCode::SuperLeft)
        || keyboard.pressed(KeyCode::SuperRight)
        || keyboard.pressed(KeyCode::ControlLeft)
        || keyboard.pressed(KeyCode::ControlRight);
    if !ctrl {
        return;
    }

    copy_selection(sel, buffer);
}
