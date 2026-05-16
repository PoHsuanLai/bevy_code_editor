//! Editor-specific mouse interactions, observer-driven via `bevy_picking`.
//!
//! The plain-click cursor placement, drag-extend selection, and scroll wheel
//! handling are owned by `bevy_instanced_text_editor`'s picking observers (which apply
//! to any `TextView` entity). The editor adds **modifier-click** behavior on
//! top: alt-click adds a secondary cursor, ctrl-click triggers LSP
//! goto-definition, and the fold-gutter strip toggles fold regions.
//!
//! Each behavior lives in its own observer; there's no monolithic mouse
//! handler. Selection state is the unified `SelectionState.selections` —
//! `bevy_instanced_text_editor::TextViewDragState` is the unified drag-tracking
//! Component.
//!
//! LSP hover is similarly an observer on `Pointer<Move>`: when the cursor
//! lingers on a position long enough, a hover request is sent. Mouse-leave
//! resets the timer.
//!
//! All screen-to-char hit-testing flows through the fold-aware
//! `screen_to_char_pos` helper so editors with active fold regions see the
//! click land on the right buffer line.

use crate::settings::GutterConfig;
use bevy_instanced_text_editor::RopeBuffer;
use crate::text_view::TextBuffer;
use crate::types::*;
#[cfg(feature = "lsp")]
use bevy::picking::events::Move;
use bevy::picking::events::{Pointer, Press};
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use bevy::ui::ComputedNode;
use bevy::ui::ScrollPosition;
use bevy_instanced_text::{DisplayLayout, MonoCellWidth};
use ropey::Rope;

type AltClickQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut SelectionState,
        &'static mut CursorState,
        &'static TextBuffer<RopeBuffer>,
        &'static ScrollPosition,
        &'static ComputedNode,
        &'static FoldState,
        &'static TextFont,
        &'static bevy::text::LineHeight,
        &'static MonoCellWidth,
        Option<&'static DisplayLayout>,
    ),
    With<CodeEditor>,
>;

#[cfg(feature = "lsp")]
type CtrlClickQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static TextBuffer<RopeBuffer>,
        &'static ScrollPosition,
        &'static ComputedNode,
        &'static FoldState,
        &'static TextFont,
        &'static bevy::text::LineHeight,
        &'static MonoCellWidth,
        Option<&'static DisplayLayout>,
    ),
    With<CodeEditor>,
>;

#[cfg(feature = "lsp")]
type HoverMoveQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static TextBuffer<RopeBuffer>,
        &'static ScrollPosition,
        &'static ComputedNode,
        &'static FoldState,
        &'static TextFont,
        &'static bevy::text::LineHeight,
        &'static MonoCellWidth,
        Option<&'static DisplayLayout>,
        &'static crate::settings::LspConfig,
    ),
    With<CodeEditor>,
>;

#[cfg(feature = "lsp")]
use crate::lsp_ui::reset_hover_state;
#[cfg(feature = "lsp")]
use bevy_lsp::LspMessage;

/// Read-only context for the fold-aware screen→char hit-test.
struct HitTestCtx<'a> {
    rope: &'a Rope,
    layout: Option<&'a DisplayLayout>,
    mono: &'a MonoCellWidth,
    line_height: f32,
    text_area_left: f32,
    text_area_top: f32,
    fold_state: &'a FoldState,
    scroll_y: f32,
}

/// Convert a viewport-local pixel position to a character index in the rope,
/// honoring fold-state's display-row → buffer-line mapping. Used by every
/// editor mouse observer that needs to know which character was clicked.
fn screen_to_char_pos(screen_pos: Vec2, ctx: &HitTestCtx<'_>) -> usize {
    let relative_x = screen_pos.x - ctx.text_area_left;
    // scroll_y is positive-downward: add to shift content up relative to viewport.
    let relative_y = screen_pos.y - ctx.text_area_top + ctx.scroll_y;

    let display_row = (relative_y / ctx.line_height).max(0.0) as usize;
    let buffer_line = ctx.fold_state.display_to_actual_line(display_row);

    let line_count = ctx.rope.len_lines();
    if buffer_line >= line_count {
        return ctx.rope.len_chars();
    }

    let line_start_char = ctx.rope.line_to_char(buffer_line);

    if let Some(layout) = ctx.layout {
        if let Some(byte_in_row) = layout.byte_at_x(display_row as u32, relative_x) {
            let row = layout
                .lines
                .iter()
                .find(|l| l.display_row == display_row as u32);
            let row_buffer_line = row.map(|r| r.buffer_row as usize).unwrap_or(buffer_line);
            let buffer_byte_offset = row.map(|r| r.buffer_byte_offset).unwrap_or(0);
            let line_start_byte = ctx.rope.line_to_byte(row_buffer_line);
            let line_end_byte = if row_buffer_line + 1 < line_count {
                ctx.rope.line_to_byte(row_buffer_line + 1)
            } else {
                ctx.rope.len_bytes()
            };
            let abs_byte = (line_start_byte + buffer_byte_offset + byte_in_row).min(line_end_byte);
            return ctx.rope.byte_to_char(abs_byte);
        }
    }

    let col = (relative_x / ctx.mono.px).max(0.0) as usize;
    let line_len = ctx.rope.line(buffer_line).len_chars().saturating_sub(1);
    let char_in_line = col.min(line_len);
    line_start_char + char_in_line
}

/// Fold-gutter click observer: toggle fold regions when the click lands in
/// the narrow strip just before the gutter separator.
#[cfg(feature = "tree-sitter")]
pub fn on_fold_gutter_press(
    trigger: On<Pointer<Press>>,
    mut editor_query: Query<
        (&ScrollPosition, &ComputedNode, &GutterConfig, &mut FoldState, &TextFont, &bevy::text::LineHeight, &MonoCellWidth),
        With<CodeEditor>,
    >,
) {
    if trigger.event().button != PointerButton::Primary {
        return;
    }
    let entity = trigger.event().entity;
    let Ok((scroll, computed, gutter, mut fold_state, font, lh, _mono)) = editor_query.get_mut(entity) else {
        return;
    };
    let Some(local_pos) = trigger.event().hit.position.map(|p| Vec2::new(p.x, p.y)) else {
        return;
    };

    let gutter_start = gutter.gutter_width - 18.0;
    let gutter_end = gutter.gutter_width + 5.0;
    if local_pos.x < gutter_start || local_pos.x >= gutter_end {
        return;
    }

    let line_height = bevy_instanced_text::resolve_line_height(*lh, font.font_size);
    let inv = computed.inverse_scale_factor();
    let text_area_top = computed.content_inset().min_inset.y * inv;
    let relative_y = local_pos.y - text_area_top + scroll.y;
    let display_row = (relative_y / line_height).max(0.0) as usize;
    let buffer_line = fold_state.display_to_actual_line(display_row);

    if fold_state.is_foldable_line(buffer_line) {
        fold_state.toggle_fold_at_line(buffer_line);
    }
}

/// Alt+click observer: add a secondary cursor at the clicked character.
///
/// `bevy_instanced_text_interaction::on_pointer_press` already skips writing
/// selection when Alt is held, so this observer owns the alt-click semantic
/// exclusively — no fight with the plain-click path.
#[allow(clippy::too_many_arguments)]
pub fn on_alt_click(
    trigger: On<Pointer<Press>>,
    mut editor_query: AltClickQuery,
    keyboard: Res<ButtonInput<KeyCode>>,
    #[cfg(feature = "lsp")] mut lsp_query: Query<
        &mut crate::lsp_ui::state::LspHoverPopup,
        With<CodeEditor>,
    >,
) {
    if trigger.event().button != PointerButton::Primary {
        return;
    }
    if !(keyboard.pressed(KeyCode::AltLeft) || keyboard.pressed(KeyCode::AltRight)) {
        return;
    }
    let entity = trigger.event().entity;
    let Ok((mut sel, mut cursor, buffer, scroll, computed, fold_state, font, lh, mono, layout)) =
        editor_query.get_mut(entity)
    else {
        return;
    };
    let Some(local_pos) = trigger.event().hit.position.map(|p| Vec2::new(p.x, p.y)) else {
        return;
    };

    let inv = computed.inverse_scale_factor();
    let char_pos = screen_to_char_pos(
        local_pos,
        &HitTestCtx {
            rope: buffer.rope(),
            layout,
            mono,
            line_height: bevy_instanced_text::resolve_line_height(*lh, font.font_size),
            text_area_left: computed.content_inset().min_inset.x * inv,
            text_area_top: computed.content_inset().min_inset.y * inv,
            fold_state,
            scroll_y: scroll.y,
        },
    );

    sel.add_cursor_at(&**buffer, char_pos);
    sel.refresh_primary_cursor(&mut cursor);

    #[cfg(feature = "lsp")]
    {
        if let Ok(mut hover_state) = lsp_query.get_mut(entity) {
            reset_hover_state(&mut hover_state);
        }
    }
}

/// Ctrl+click observer: trigger an LSP `goto-definition` at the clicked
/// character. Editor crate only; under `feature = "lsp"`.
#[cfg(feature = "lsp")]
#[allow(clippy::too_many_arguments)]
pub fn on_ctrl_click_goto_definition(
    trigger: On<Pointer<Press>>,
    editor_query: CtrlClickQuery,
    mut lsp_query: Query<(&mut bevy_lsp::LspClient, Option<&bevy_lsp::LspDocument>), With<CodeEditor>>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    if trigger.event().button != PointerButton::Primary {
        return;
    }
    if !(keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight)) {
        return;
    }
    let entity = trigger.event().entity;
    let Ok((buffer, scroll, computed, fold_state, font, lh, mono, layout)) = editor_query.get(entity) else {
        return;
    };
    let Ok((mut lsp_client, lsp_document)) = lsp_query.get_mut(entity) else {
        return;
    };
    let Some(doc) = lsp_document else {
        return;
    };
    let Some(local_pos) = trigger.event().hit.position.map(|p| Vec2::new(p.x, p.y)) else {
        return;
    };

    let inv = computed.inverse_scale_factor();
    let char_pos = screen_to_char_pos(
        local_pos,
        &HitTestCtx {
            rope: buffer.rope(),
            layout,
            mono,
            line_height: bevy_instanced_text::resolve_line_height(*lh, font.font_size),
            text_area_left: computed.content_inset().min_inset.x * inv,
            text_area_top: computed.content_inset().min_inset.y * inv,
            fold_state,
            scroll_y: scroll.y,
        },
    );

    let lsp_position = bevy_lsp::rope_char_to_lsp_position(
        buffer.rope(),
        char_pos,
        bevy_lsp::PositionEncoding::Utf16,
    );
    lsp_client.send(LspMessage::GotoDefinition {
        uri: doc.uri.clone(),
        position: lsp_position,
        id: 0,
    });
}

/// LSP hover-trigger observer: arms a delay timer when the pointer moves to
/// a new character, then fires a `Hover` request when the timer elapses.
/// Editor crate only; under `feature = "lsp"`.
///
/// The timer-tick lives in a separate system (`tick_lsp_hover_timer`) since
/// observers don't see `Time`. The observer writes the new char position +
/// resets the timer; the system advances it.
#[cfg(feature = "lsp")]
pub fn on_pointer_move_for_hover(
    trigger: On<Pointer<Move>>,
    editor_query: HoverMoveQuery,
    mut hover_query: Query<&mut crate::lsp_ui::state::LspHoverPopup, With<CodeEditor>>,
) {
    let entity = trigger.event().entity;
    let Ok((buffer, scroll, computed, fold_state, font, lh, mono, layout, hover_settings)) =
        editor_query.get(entity)
    else {
        return;
    };
    if !hover_settings.hover.enabled {
        return;
    }
    let Ok(mut hover_state) = hover_query.get_mut(entity) else {
        return;
    };
    let Some(local_pos) = trigger.event().hit.position.map(|p| Vec2::new(p.x, p.y)) else {
        return;
    };

    // Bail out before the rope/layout hit-test if the pointer has barely moved
    // in screen space since the last trigger — saves the per-event work on
    // sub-pixel jitter and at-rest cursors. Threshold is one char width.
    if let Some(last) = hover_state.last_pointer_pos {
        if (last - local_pos).length_squared() < (mono.px * mono.px) {
            return;
        }
    }
    hover_state.last_pointer_pos = Some(local_pos);

    let inv = computed.inverse_scale_factor();
    let line_height = bevy_instanced_text::resolve_line_height(*lh, font.font_size);
    let char_pos = screen_to_char_pos(
        local_pos,
        &HitTestCtx {
            rope: buffer.rope(),
            layout,
            mono,
            line_height,
            text_area_left: computed.content_inset().min_inset.x * inv,
            text_area_top: computed.content_inset().min_inset.y * inv,
            fold_state,
            scroll_y: scroll.y,
        },
    );

    if hover_state.trigger_char_index != char_pos {
        hover_state.trigger_char_index = char_pos;
        hover_state.timer = Some(Timer::new(
            std::time::Duration::from_millis(hover_settings.hover.delay_ms),
            TimerMode::Once,
        ));
        hover_state.visible = false;
        hover_state.request_sent = false;
    }
}

/// LSP hover-out observer: resets hover state when the pointer leaves the
/// editor entity, so the popup doesn't pop after the cursor has moved away.
#[cfg(feature = "lsp")]
pub fn on_pointer_out_for_hover(
    trigger: On<bevy::picking::events::Pointer<bevy::picking::events::Out>>,
    mut hover_query: Query<&mut crate::lsp_ui::state::LspHoverPopup, With<CodeEditor>>,
) {
    let entity = trigger.event().entity;
    if let Ok(mut hover_state) = hover_query.get_mut(entity) {
        reset_hover_state(&mut hover_state);
    }
}

/// Tick the per-editor LSP hover delay timer. When the timer elapses on an
/// armed entity (one with `trigger_char_index` set by the move observer),
/// fire a `Hover` request to the LSP server.
///
/// Editor crate only; under `feature = "lsp"`.
#[cfg(feature = "lsp")]
pub fn tick_lsp_hover_timer(
    editor_query: Query<&TextBuffer<RopeBuffer>, With<CodeEditor>>,
    mut state_query: Query<
        (
            Entity,
            &mut bevy_lsp::LspClient,
            Option<&bevy_lsp::LspDocument>,
            &mut crate::lsp_ui::state::LspHoverPopup,
        ),
        With<CodeEditor>,
    >,
    time: Res<Time>,
) {
    for (entity, mut lsp_client, lsp_document, mut hover_state) in state_query.iter_mut() {
        let Ok(buffer) = editor_query.get(entity) else {
            continue;
        };
        let Some(timer) = hover_state.timer.as_mut() else {
            continue;
        };
        timer.tick(time.delta());
        if !timer.just_finished() || hover_state.request_sent {
            continue;
        }
        let Some(doc) = lsp_document else { continue };

        // Clamp to last char of line (exclude newline).
        let current_char_pos = hover_state.trigger_char_index.min(buffer.len_chars());
        let line_index = buffer.char_to_line(current_char_pos);
        let line_start = buffer.line_to_char(line_index);
        let line_len = buffer.line(line_index).len_chars();
        let clamped = line_start + (current_char_pos - line_start).min(line_len.saturating_sub(1));
        let lsp_position = bevy_lsp::rope_char_to_lsp_position(
            buffer.rope(),
            clamped,
            bevy_lsp::PositionEncoding::Utf16,
        );

        lsp_client.send(LspMessage::Hover {
            uri: doc.uri.clone(),
            position: lsp_position,
            id: 0,
        });
        hover_state.request_sent = true;
        hover_state.pending_char_index = Some(current_char_pos);
    }
}
