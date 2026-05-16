//! Cursor rendering and animation.

use crate::settings::{CursorLine, CursorSettings, EditorTheme};
use bevy_instanced_text_editor::RopeBuffer;
use crate::text_view::{DisplayLayout, RectOverlay, RowVertical, TextBuffer};
use bevy::ui::ComputedNode;
use crate::types::*;
use bevy::prelude::*;
use bevy_instanced_text::MonoCellWidth;

type PushCursorOverlaysQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static SelectionState,
        &'static CursorState,
        &'static bevy_instanced_text_editor::BlinkPhase,
        &'static TextBuffer<RopeBuffer>,
        &'static mut CaretRects,
        &'static FoldState,
        &'static MonoCellWidth,
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
        &'static TextBuffer<RopeBuffer>,
        &'static ComputedNode,
        &'static mut CursorLineRects,
        &'static FoldState,
        &'static MonoCellWidth,
        Option<&'static DisplayLayout>,
        &'static EditorTheme,
        &'static CursorLine,
    ),
    With<CodeEditor>,
>;

pub struct CursorPlugin;

impl Plugin for CursorPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<crate::types::events::CompletionApplied>()
            .register_type::<BracketMatch>()
            .register_type::<BracketMatchHighlight>()
            .register_type::<BracketMatchState>()
            .register_type::<CodeEditor>()
            .register_type::<crate::types::events::CompletionDismissed>()
            .register_type::<EditorCursor>()
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
            .register_type::<super::editor_ui::AutoResizeViewport>()
            .register_type::<crate::input::EditorAction>()
            .register_type::<GutterTextView>();

        app.register_type::<crate::settings::BracketHighlightStyle>()
            .register_type::<crate::settings::BracketConfig>()
            .register_type::<crate::settings::GutterConfig>()
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
        app.add_systems(PostUpdate, push_cursor_overlays.in_set(super::RenderingSet));
    }
}

pub(crate) fn track_cursor_movement(
    mut editor_query: Query<
        (&mut CursorState, &mut bevy_instanced_text_editor::BlinkPhase),
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

pub(crate) fn push_cursor_overlays(
    mut editor_query: PushCursorOverlaysQuery,
    input_focus: Res<bevy::input_focus::InputFocus>,
    time: Res<Time>,
) {
    for (entity, sel, cursor, blink, buffer, mut carets, fold_state, mono, layout, theme, cursor_settings) in
        editor_query.iter_mut()
    {
        let focused = input_focus.get() == Some(entity);
        let visible = focused
            && bevy_instanced_text_editor::cursor_blink_visible(
                cursor_settings.blink_rate,
                cursor_settings.blink_pause_secs,
                time.elapsed_secs_f64(),
                blink.last_change_secs,
            );

        let mut new_rects: Vec<RectOverlay> = Vec::new();
        if visible {
            let char_width = mono.px;
            let _ = cursor;
            for selection in sel.selections.iter() {
                let cursor_pos = selection.head_offset().min(buffer.len_chars());
                let line_index = buffer.char_to_line(cursor_pos);
                let line_start = buffer.line_to_char(line_index);
                let col_index = cursor_pos - line_start;
                let line = buffer.line(line_index);
                let col_clamped = col_index.min(line.len_chars());
                let byte_in_line = line.slice(..col_clamped).len_bytes();
                let (display_row, byte_in_row) = layout
                    .and_then(|l| l.buffer_to_display(line_index as u32, byte_in_line))
                    .map(|(r, b)| (r as usize, b))
                    .unwrap_or_else(|| (fold_state.actual_to_display_line(line_index), byte_in_line));
                let glyph_x = layout.and_then(|l| l.x_at_byte(display_row as u32, byte_in_row));
                let x_left = glyph_x.unwrap_or(col_index as f32 * char_width);
                new_rects.push(bevy_instanced_text_editor::caret_overlay(
                    display_row as u32,
                    x_left,
                    cursor_settings,
                    theme.cursor,
                ));
            }
        }

        if carets.0 != new_rects {
            carets.0 = new_rects;
        }
    }
}

pub(crate) fn update_cursor_line_highlight(mut editor_query: CursorLineHighlightQuery) {
    for (sel, cursor, buffer, computed, mut cursor_line_rects, fold_state, mono, layout, theme, cursor_line) in
        editor_query.iter_mut()
    {
        if !cursor_line.enabled || theme.line_highlight.is_none() {
            if !cursor_line_rects.0.is_empty() {
                cursor_line_rects.0.clear();
            }
            continue;
        }

        let char_width = mono.px;
        let border_thickness = cursor_line.border_thickness;
        let border_color = cursor_line.border_color;
        let word_highlight_color = cursor_line.word_highlight_color;
        let inv = computed.inverse_scale_factor();
        let text_area_left = computed.content_inset().min_inset.x * inv;
        let viewport_width = computed.size().x * inv;
        let band_x_left = -text_area_left;
        let band_x_right = viewport_width - text_area_left;
        let _ = cursor;

        let mut new_rects: Vec<RectOverlay> = Vec::new();
        for selection in sel.selections.iter() {
            let cursor_pos = selection.head_offset().min(buffer.len_chars());
            let line_index = buffer.char_to_line(cursor_pos);
            if fold_state.is_line_hidden(line_index) { continue; }

            let line_start = buffer.line_to_char(line_index);
            let col_in_line = cursor_pos - line_start;
            let line_for_byte = buffer.line(line_index);
            let col_clamped = col_in_line.min(line_for_byte.len_chars());
            let cursor_byte = line_for_byte.slice(..col_clamped).len_bytes();
            let display_row = layout
                .and_then(|l| l.buffer_to_display(line_index as u32, cursor_byte))
                .map(|(r, _)| r as usize)
                .unwrap_or_else(|| fold_state.actual_to_display_line(line_index));

            if cursor_line.show_border {
                new_rects.push(RectOverlay {
                    display_row: display_row as u32,
                    x_range: band_x_left..band_x_right,
                    vertical: RowVertical::TopBand { thickness: border_thickness },
                    color: border_color,
                    z: 0,
                    corners: bevy_instanced_text::CornerRadii::ZERO,
                });
                new_rects.push(RectOverlay {
                    display_row: display_row as u32,
                    x_range: band_x_left..band_x_right,
                    vertical: RowVertical::BottomBand { thickness: border_thickness },
                    color: border_color,
                    z: 0,
                    corners: bevy_instanced_text::CornerRadii::ZERO,
                });
            }

            if !cursor_line.highlight_word { continue; }

            let col = cursor_pos - line_start;
            let line = buffer.line(line_index);
            let line_chars: Vec<char> = line.chars().collect();
            let is_word_char = |ch: char| ch.is_alphanumeric() || ch == '_';
            let on_word = if col < line_chars.len() && is_word_char(line_chars[col]) {
                true
            } else {
                col > 0 && col <= line_chars.len() && is_word_char(line_chars[col - 1])
            };
            let (word_start, word_end) = if on_word {
                let start_col = if col < line_chars.len() && is_word_char(line_chars[col]) { col } else { col - 1 };
                let mut ws = start_col;
                while ws > 0 && is_word_char(line_chars[ws - 1]) { ws -= 1; }
                let mut we = start_col;
                while we < line_chars.len() && is_word_char(line_chars[we]) { we += 1; }
                (ws, we)
            } else {
                (col, col)
            };

            if word_end > word_start {
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
                    let xl = layout.and_then(|l| l.x_at_byte(start_row, start_byte)).unwrap_or(word_start as f32 * char_width);
                    let xr = layout.and_then(|l| l.x_at_byte(end_row, end_byte)).unwrap_or(word_end as f32 * char_width);
                    new_rects.push(RectOverlay {
                        display_row: start_row,
                        x_range: xl..xr,
                        vertical: RowVertical::Full,
                        color: word_highlight_color,
                        z: 0,
                        corners: bevy_instanced_text::CornerRadii::ZERO,
                    });
                } else {
                    let xl = layout.and_then(|l| l.x_at_byte(start_row, start_byte)).unwrap_or(word_start as f32 * char_width);
                    new_rects.push(RectOverlay {
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

        if cursor_line_rects.0 != new_rects {
            cursor_line_rects.0 = new_rects;
        }
    }
}
