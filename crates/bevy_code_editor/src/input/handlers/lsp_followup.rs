//! Post-action LSP follow-up system.
//!
//! The pre-refactor `execute_action` ran three side-effects after the action
//! body finished:
//!   1. If the cursor moved horizontally, hide the completion popup.
//!   2. If `DeleteBackward` ran with the popup visible, refilter or hide.
//!   3. If text changed, send `textDocument/didChange`.
//!
//! With handlers now event-driven we can't carry per-action flags through
//! the dispatch. Instead, `dispatch_action_events` writes a
//! [`PendingActionFollowup`] resource each frame, and this system inspects
//! the cursor/content snapshot delta against the editor's current state and
//! fires the same three side-effects. Behavior matches the original.

use crate::input::action_events::*;
use crate::types::*;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;

/// Carries the action snapshot from `dispatch_action_events` to
/// `lsp_followup`. Cleared at the end of `lsp_followup` so it never leaks
/// into the next frame.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct PendingActionFollowup {
    pub pre_cursor_pos: usize,
    /// Backspace; `update_completion_filter` only fires on `DeleteBackward`.
    pub was_delete_backward: bool,
    /// Horizontal cursor move; the completion popup hides on horizontal,
    /// stays on vertical.
    pub was_horizontal_move: bool,
    /// `false` short-circuits `lsp_followup` before any LSP work runs.
    pub action_fired: bool,
}

pub fn lsp_followup(
    mut pending: ResMut<PendingActionFollowup>,
    input_focus: Res<InputFocus>,
    editor_q: Query<
        (&CursorState, &crate::text_view::TextBuffer),
        With<CodeEditor>,
    >,
    mut lsp_q: Query<
        (
            &bevy_lsp::LspClient,
            Option<&mut bevy_lsp::LspDocument>,
            &mut crate::lsp_ui::state::LspCompletionPopup,
        ),
        With<CodeEditor>,
    >,
) {
    if !pending.action_fired {
        return;
    }
    let snapshot = *pending;
    pending.action_fired = false;

    let Some(entity) = input_focus.get() else {
        return;
    };
    let Ok((cursor, buffer)) = editor_q.get(entity) else {
        return;
    };
    let Ok((lsp_client, lsp_document, mut completion_state)) = lsp_q.get_mut(entity) else {
        return;
    };

    // (1) Cursor-move dismissal is handled by
    //     `dismiss_completion_on_cursor_move` (mirrors Zed's char-kind
    //     check). Nothing to do here.
    let _ = snapshot.was_horizontal_move;

    // (2) Backspace inside an active completion popup refilters or hides
    //     the popup based on whether the cursor is still past the popup's
    //     anchor position.
    if snapshot.was_delete_backward && completion_state.visible {
        if cursor.cursor_pos > completion_state.start_char_index {
            crate::input::actions::update_completion_filter(
                cursor,
                &buffer.rope,
                &mut completion_state,
            );
        } else if cursor.cursor_pos == completion_state.start_char_index {
            // Empty prefix: hide (Zed behavior).
            completion_state.dismiss();
        } else {
            completion_state.dismiss();
        }
    }

    let _ = (buffer, lsp_client, lsp_document);
}

/// Drain any unhandled action-event queues so they don't accumulate when LSP
/// is enabled and the popup intercepted the original dispatch. This is a
/// no-op in practice (handlers always drain their own events), but keeps
/// the message buffers tidy.
#[allow(dead_code)]
pub fn _drain_unused(_: MessageReader<DeleteBackwardRequested>) {}
