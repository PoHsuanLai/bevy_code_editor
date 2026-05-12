//! Cursor rendering and animation.
//!
//! Cursor carets are pushed into `TextViewOverlays` as `RectOverlay`s; the
//! engine's renderer paints them with the rest of the layer. Blink animation
//! lives inside `push_cursor_overlays` itself — no separate `animate_cursor`
//! system.

use crate::settings::{CursorLine, CursorSettings, EditorTheme};
use crate::text_view::{
    DisplayLayout, RectOverlay, RowVertical, TextBuffer, TextViewOverlays, TextViewport,
};
use crate::types::*;
use bevy::prelude::*;
use bevy_instanced_text::TextFont;

type PushCursorOverlaysQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static SelectionState,
        &'static CursorState,
        &'static bevy_instanced_text_edit::BlinkPhase,
        &'static TextBuffer,
        &'static TextViewport,
        &'static mut TextViewOverlays,
        &'static FoldState,
        &'static TextFont,
        Option<&'static DisplayLayout>,
        &'static EditorTheme,
        &'static CursorSettings,
    ),
    With<CodeEditor>,
>;

type CursorLineHighlightQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static SelectionState,
        &'static CursorState,
        &'static TextBuffer,
        &'static TextViewport,
        &'static mut TextViewOverlays,
        &'static FoldState,
        &'static TextFont,
        Option<&'static DisplayLayout>,
        &'static EditorTheme,
        &'static CursorLine,
    ),
    With<CodeEditor>,
>;

pub struct CursorPlugin;

impl Plugin for CursorPlugin {
    fn build(&self, app: &mut App) {
        // Group register_type calls for the editor's pure-data Component /
        // Resource / Message types here. Internal state Components carrying
        // cosmic-text / tree-sitter / lsp_types fields stay non-reflectable
        // and are documented at their definition sites.
        app.register_type::<crate::types::events::CompletionApplied>()
            .register_type::<BracketMatch>()
            .register_type::<BracketMatchHighlight>()
            .register_type::<BracketMatchState>()
            .register_type::<CodeEditor>()
            .register_type::<crate::types::events::CompletionDismissed>()
            .register_type::<EditorCursor>()
            .register_type::<IndentGuide>()
            .register_type::<KeyRepeatState>()
            .register_type::<LineNumbers>()
            .register_type::<OpenRequested>()
            .register_type::<crate::types::events::CompletionRequested>()
            .register_type::<crate::types::events::HoverRequested>()
            .register_type::<crate::types::events::RenameRequested>()
            .register_type::<crate::types::events::SignatureHelpRequested>()
            .register_type::<SaveRequested>()
            .register_type::<SelectionHighlight>()
            .register_type::<Separator>()
            .register_type::<crate::types::events::TextEdited>()
            .register_type::<super::editor_ui_plugin::AutoResizeViewport>()
            .register_type::<crate::input::EditorAction>()
            .register_type::<super::gpu_line_numbers::GpuLineNumbersBatch>();

        // Settings resources.
        app.register_type::<crate::settings::BracketHighlightStyle>()
            .register_type::<crate::settings::BracketConfig>()
            .register_type::<crate::settings::CursorLine>()
            .register_type::<crate::settings::CursorLineStyle>()
            .register_type::<crate::settings::CursorSettings>()
            .register_type::<crate::settings::CursorStyle>()
            .register_type::<crate::settings::Indentation>()
            .register_type::<crate::settings::KeyRepeatSettings>()
            .register_type::<crate::settings::Performance>()
            .register_type::<crate::settings::SyntaxColors>()
            .register_type::<crate::settings::EditorTheme>()
            .register_type::<crate::settings::EditorUi>()
            .register_type::<crate::settings::WhitespaceMode>()
            .register_type::<crate::settings::Wrapping>();

        #[cfg(feature = "lsp")]
        app.register_type::<crate::settings::LspConfig>();

        app.add_systems(Update, track_cursor_movement.in_set(super::ApplyStateSet));

        // Caret rects are pushed into `TextViewOverlays` during `RenderingSet`,
        // which runs after the display-map build and before the actual paint.
        app.add_systems(Update, push_cursor_overlays.in_set(super::RenderingSet));

        // Note: update_cursor_line_highlight is registered by EditorUiPlugin
        // where it's chained with other visual systems.
    }
}

/// Uses last_cursor_pos_for_blink (separate from last_cursor_pos) to avoid
/// race conditions with auto_scroll_to_cursor.
pub(crate) fn track_cursor_movement(
    mut editor_query: Query<
        (&mut CursorState, &mut bevy_instanced_text_edit::BlinkPhase),
        With<CodeEditor>,
    >,
    time: Res<Time>,
) {
    for (mut cursor, mut blink) in editor_query.iter_mut() {
        let current_pos = cursor.cursor_pos;
        if current_pos != cursor.last_cursor_pos_for_blink {
            blink.last_change_secs = time.elapsed_secs_f64();
            cursor.last_cursor_pos_for_blink = current_pos;
        }
    }
}

/// Push caret rectangles into `TextViewOverlays` for each cursor.
///
/// Blink and position collapse into one system that skips pushing during the
/// off-phase of the blink cycle.
///
/// This is a *partial* writer of `TextViewOverlays` — selection and other
/// overlay producers push too, so each producer drains only its own rects
/// (identified by `z`: caret = +1, line-highlight = 0, selection = -1)
/// before pushing fresh ones.
pub(crate) fn push_cursor_overlays(
    mut editor_query: PushCursorOverlaysQuery,
    input_focus: Res<bevy::input_focus::InputFocus>,
    time: Res<Time>,
) {
    for (
        entity,
        sel,
        cursor,
        blink,
        buffer,
        _vp,
        mut overlays,
        fold_state,
        font,
        layout,
        theme,
        cursor_settings,
    ) in editor_query.iter_mut()
    {
        let focused = input_focus.get() == Some(entity);
        // Drain any caret rects from the previous frame. We mark them with
        // `z = +1` so we can identify them; selection rects use `z = -1` and
        // line-highlight uses `z = 0`.
        overlays.rects.retain(|r| r.z != 1);

        if !focused
            || !bevy_instanced_text_edit::cursor_blink_visible(
                cursor_settings.blink_rate,
                cursor_settings.blink_pause_secs,
                time.elapsed_secs_f64(),
                blink.last_change_secs,
            )
        {
            overlays.version = overlays.version.wrapping_add(1);
            continue;
        }

        let char_width = font.char_width;
        let _ = cursor; // unused under the SelectionCollection-driven path

        for selection in sel.selections.iter() {
            let cursor_pos = selection.head_offset().min(buffer.rope.len_chars());
            let line_index = buffer.rope.char_to_line(cursor_pos);
            let line_start = buffer.rope.line_to_char(line_index);
            let col_index = cursor_pos - line_start;

            // Convert to display coordinates via the layout. With wrap on,
            // multiple display rows may share a buffer line; the layout's
            // `buffer_to_display` walks them. With wrap off, fold-state still
            // gives the right answer for off-viewport rows.
            let line = buffer.rope.line(line_index);
            let col_clamped = col_index.min(line.len_chars());
            let byte_in_line = line.slice(..col_clamped).len_bytes();
            let (display_row, byte_in_row) = layout
                .and_then(|l| l.buffer_to_display(line_index as u32, byte_in_line))
                .map(|(r, b)| (r as usize, b))
                .unwrap_or_else(|| (fold_state.actual_to_display_line(line_index), byte_in_line));

            let glyph_x = layout.and_then(|l| l.x_at_byte(display_row as u32, byte_in_row));
            let x_left = glyph_x.unwrap_or(col_index as f32 * char_width);

            overlays.rects.push(bevy_instanced_text_edit::caret_overlay(
                display_row as u32,
                x_left,
                cursor_settings,
                theme.cursor,
            ));
        }

        overlays.version = overlays.version.wrapping_add(1);
    }
}
pub(crate) fn update_cursor_line_highlight(mut editor_query: CursorLineHighlightQuery) {
    for (sel, cursor, buffer, vp, mut overlays, fold_state, font, layout, theme, cursor_line) in
        editor_query.iter_mut()
    {
        // Drain previous-frame line-border / word rects (z = 0 reserved for cursor-line decoration).
        overlays.rects.retain(|r| r.z != 0);

        if !cursor_line.enabled || theme.line_highlight.is_none() {
            overlays.version = overlays.version.wrapping_add(1);
            continue;
        }

        let char_width = font.char_width;

        let border_thickness = cursor_line.border_thickness;
        let border_color = cursor_line.border_color;
        let word_highlight_color = cursor_line.word_highlight_color;

        // Full-line-width band, in pixels relative to the row's text origin.
        // text_area_left is already the row's "x = 0" anchor in render_layout, so the
        // band stretches from the negative gutter edge to the viewport's right edge.
        let band_x_left = -vp.text_area_left;
        let band_x_right = vp.width as f32 - vp.text_area_left;
        let _ = cursor; // legacy field kept for blink tracking; iteration uses `sel`

        for selection in sel.selections.iter() {
            let cursor_pos = selection.head_offset().min(buffer.rope.len_chars());
            let line_index = buffer.rope.char_to_line(cursor_pos);

            if fold_state.is_line_hidden(line_index) {
                continue;
            }

            let line_start = buffer.rope.line_to_char(line_index);
            let col_in_line = cursor_pos - line_start;
            let line_for_byte = buffer.rope.line(line_index);
            let col_clamped = col_in_line.min(line_for_byte.len_chars());
            let cursor_byte = line_for_byte.slice(..col_clamped).len_bytes();
            let display_row = layout
                .and_then(|l| l.buffer_to_display(line_index as u32, cursor_byte))
                .map(|(r, _)| r as usize)
                .unwrap_or_else(|| fold_state.actual_to_display_line(line_index));

            if cursor_line.show_border {
                overlays.rects.push(RectOverlay {
                    display_row: display_row as u32,
                    x_range: band_x_left..band_x_right,
                    vertical: RowVertical::TopBand {
                        thickness: border_thickness,
                    },
                    color: border_color,
                    z: 0,
                    corners: bevy_instanced_text::CornerRadii::ZERO,
                });
                overlays.rects.push(RectOverlay {
                    display_row: display_row as u32,
                    x_range: band_x_left..band_x_right,
                    vertical: RowVertical::BottomBand {
                        thickness: border_thickness,
                    },
                    color: border_color,
                    z: 0,
                    corners: bevy_instanced_text::CornerRadii::ZERO,
                });
            }

            if !cursor_line.highlight_word {
                continue;
            }

            let line_start = buffer.rope.line_to_char(line_index);
            let col = cursor_pos - line_start;
            let line = buffer.rope.line(line_index);
            let line_chars: Vec<char> = line.chars().collect();
            let is_word_char = |ch: char| ch.is_alphanumeric() || ch == '_';
            let on_word = if col < line_chars.len() && is_word_char(line_chars[col]) {
                true
            } else {
                col > 0 && col <= line_chars.len() && is_word_char(line_chars[col - 1])
            };
            let (word_start, word_end) = if on_word {
                let start_col = if col < line_chars.len() && is_word_char(line_chars[col]) {
                    col
                } else {
                    col - 1
                };
                let mut ws = start_col;
                while ws > 0 && is_word_char(line_chars[ws - 1]) {
                    ws -= 1;
                }
                let mut we = start_col;
                while we < line_chars.len() && is_word_char(line_chars[we]) {
                    we += 1;
                }
                (ws, we)
            } else {
                (col, col)
            };

            if word_end > word_start {
                // Translate the word's char-range to bytes, then to (display_row,
                // byte_in_row) for each endpoint. With wrap on, a word spanning
                // a soft break lands on different rows; emit one rect per row.
                let ws_clamped = word_start.min(line_for_byte.len_chars());
                let we_clamped = word_end.min(line_for_byte.len_chars());
                let ws_byte = line_for_byte.slice(..ws_clamped).len_bytes();
                let we_byte = line_for_byte.slice(..we_clamped).len_bytes();
                let (start_row, start_byte) = layout
                    .and_then(|l| l.buffer_to_display(line_index as u32, ws_byte))
                    .unwrap_or((display_row as u32, ws_byte));
                let (end_row, end_byte) = layout
                    .and_then(|l| l.buffer_to_display(line_index as u32, we_byte))
                    .unwrap_or((display_row as u32, we_byte));
                if start_row == end_row {
                    let xl = layout
                        .and_then(|l| l.x_at_byte(start_row, start_byte))
                        .unwrap_or(word_start as f32 * char_width);
                    let xr = layout
                        .and_then(|l| l.x_at_byte(end_row, end_byte))
                        .unwrap_or(word_end as f32 * char_width);
                    overlays.rects.push(RectOverlay {
                        display_row: start_row,
                        x_range: xl..xr,
                        vertical: RowVertical::Full,
                        color: word_highlight_color,
                        z: 0,
                        corners: bevy_instanced_text::CornerRadii::ZERO,
                    });
                } else {
                    // Multi-row word (rare in practice — only happens if a wrap
                    // break lands inside a word). Highlight just the start-row
                    // portion to its right edge; skip continuation rows.
                    let xl = layout
                        .and_then(|l| l.x_at_byte(start_row, start_byte))
                        .unwrap_or(word_start as f32 * char_width);
                    overlays.rects.push(RectOverlay {
                        display_row: start_row,
                        x_range: xl..f32::MAX,
                        vertical: RowVertical::Full,
                        color: word_highlight_color,
                        z: 0,
                        corners: bevy_instanced_text::CornerRadii::ZERO,
                    });
                }
            }
        }

        overlays.version = overlays.version.wrapping_add(1);
    }
}
