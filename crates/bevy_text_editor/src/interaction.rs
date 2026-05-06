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

use bevy_text_engine::{DisplayLayout, FontConfig, TextView, TextViewState, TextViewViewport};

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
/// The unified `SelectionState` (with multi-selection support) is the
/// source of truth — only the primary selection is copied. Bare-`TextView`
/// consumers that want copy without `TextEditor` should attach a
/// `SelectionState` Component to their entity (it's a cheap default).
pub fn copy_selection(sel: &SelectionState, tv: &TextViewState) -> bool {
    let Some((start, end)) = sel.primary_range() else {
        return false;
    };
    let start = start.min(tv.rope.len_chars());
    let end = end.min(tv.rope.len_chars());
    if start == end {
        return false;
    }
    let text = tv.rope.slice(start..end).to_string();
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        let _ = clipboard.set_text(text);
        return true;
    }
    false
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
/// available text area (via `TextViewState.max_content_width`); the
/// display-map producer maintains that field as it shapes lines.
pub fn on_pointer_scroll(
    trigger: On<Pointer<Scroll>>,
    mut views: Query<
        (
            &mut TextViewState,
            &TextViewViewport,
            &FontConfig,
            Option<&ScrollConfig>,
        ),
        With<TextView>,
    >,
) {
    let entity = trigger.event().entity;
    let Ok((mut tv, viewport, font, scroll_cfg)) = views.get_mut(entity) else {
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
        if tv.max_content_width > available_text_width {
            let scroll_delta = dx * font.char_width * scroll_cfg.speed;
            let max_h = (tv.max_content_width - available_text_width).max(0.0);
            if scroll_cfg.smooth {
                tv.target_horizontal_scroll_offset =
                    (tv.target_horizontal_scroll_offset + scroll_delta).clamp(0.0, max_h);
            } else {
                tv.horizontal_scroll_offset =
                    (tv.horizontal_scroll_offset + scroll_delta).clamp(0.0, max_h);
            }
        }
    }

    // Vertical scroll.
    if dy.abs() > 0.0 {
        let scroll_delta = dy * font.line_height * scroll_cfg.speed;
        let line_count = tv.rope.len_lines();
        let content_height = line_count as f32 * font.line_height;
        let viewport_height = viewport.height as f32;
        let max_scroll =
            (-(content_height - viewport_height + viewport.text_area_top)).min(0.0);
        if scroll_cfg.smooth {
            tv.target_scroll_offset =
                (tv.target_scroll_offset + scroll_delta).clamp(max_scroll, 0.0);
        } else {
            tv.scroll_offset = (tv.scroll_offset + scroll_delta).clamp(max_scroll, 0.0);
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
            &TextViewState,
            &TextViewViewport,
            &FontConfig,
            Option<&DisplayLayout>,
            Option<&mut SelectionState>,
            Option<&mut CursorState>,
        ),
        With<TextView>,
    >,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut input_focus: ResMut<InputFocus>,
) {
    if trigger.event().button != PointerButton::Primary {
        return;
    }
    let entity = trigger.event().entity;
    let Ok((mut drag_state, tv, viewport, font, layout, sel, cursor)) = views.get_mut(entity)
    else {
        return;
    };

    let local_pos = match trigger.event().hit.position {
        Some(p) => Vec2::new(p.x, p.y),
        None => return,
    };

    let char_pos = screen_to_char_pos(
        local_pos,
        &tv.rope,
        layout.as_deref(),
        tv.scroll_offset,
        font,
        viewport,
        None,
    );

    // Editor-feature modifiers (Alt = multi-cursor, Ctrl/Cmd = goto-definition)
    // are handled by their own observers / systems. Skip the plain-click cursor
    // move when any of those is held — the editor crate will write selection
    // state itself for the modifier path.
    let alt_held = keyboard.pressed(KeyCode::AltLeft) || keyboard.pressed(KeyCode::AltRight);
    let ctrl_held = keyboard.pressed(KeyCode::ControlLeft)
        || keyboard.pressed(KeyCode::ControlRight)
        || keyboard.pressed(KeyCode::SuperLeft)
        || keyboard.pressed(KeyCode::SuperRight);
    let modifier_held = alt_held || ctrl_held;

    if !modifier_held {
        if let Some(mut sel) = sel {
            sel.selections.set_cursor(char_pos);
        }
        if let Some(mut cursor) = cursor {
            cursor.cursor_pos = char_pos;
        }
        drag_state.is_dragging = true;
        drag_state.drag_start_pos = Some(char_pos);
        drag_state.drag_start_scroll_offset = tv.scroll_offset;
        drag_state.last_screen_pos = Some(viewport.hit_test_position + local_pos);
    }
    input_focus.set(entity);
}

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
            &TextViewState,
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
    let Ok((mut drag_state, tv, viewport, font, layout, sel, cursor)) = views.get_mut(entity)
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
        &tv.rope,
        layout.as_deref(),
        tv.scroll_offset,
        font,
        viewport,
        Some(drag_state.drag_start_scroll_offset),
    );

    if let (Some(mut sel), Some(start)) = (sel, drag_state.drag_start_pos) {
        if start == char_pos {
            sel.selections.set_cursor(char_pos);
        } else {
            sel.selections.set_selection(char_pos, start);
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
        (&SelectionState, &TextViewState),
        (With<TextView>, Without<crate::state::TextEditor>),
    >,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    let entity = trigger.event().focused_entity;
    let Ok((sel, tv)) = views.get(entity) else {
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

    copy_selection(sel, tv);
}
