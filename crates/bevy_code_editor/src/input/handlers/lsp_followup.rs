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
/// into the next frame. `pre_cursor_pos` and `pre_content_version` are
/// snapshotted before handler systems run; the followup compares them
/// against the current state to detect text/cursor changes.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct PendingActionFollowup {
    /// Cursor position before the action ran.
    pub pre_cursor_pos: usize,
    /// Content version before the action ran.
    pub pre_content_version: u64,
    /// Was the dispatched action a backspace? Required because
    /// `update_completion_filter` is only invoked on `DeleteBackward`,
    /// not on every text-change.
    pub was_delete_backward: bool,
    /// Was the dispatched action a horizontal cursor move? Required because
    /// the popup hides on horizontal moves but stays put on vertical moves.
    pub was_horizontal_move: bool,
    /// Was an action actually dispatched this frame? When `false` the
    /// followup early-returns without running any LSP work.
    pub action_fired: bool,
}

pub fn lsp_followup(
    mut pending: ResMut<PendingActionFollowup>,
    input_focus: Res<InputFocus>,
    mut editor_q: Query<
        (&CursorState, &crate::text_view::TextViewState),
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
    let Ok((cursor, tv)) = editor_q.get_mut(entity) else {
        return;
    };
    let Ok((lsp_client, mut lsp_document, mut completion_state)) = lsp_q.get_mut(entity) else {
        return;
    };

    // (1) Horizontal cursor move dismisses the completion popup.
    if snapshot.was_horizontal_move {
        completion_state.visible = false;
    }

    // (2) Backspace inside an active completion popup refilters or hides
    //     the popup based on whether the cursor is still past the popup's
    //     anchor position.
    if snapshot.was_delete_backward && completion_state.visible {
        if cursor.cursor_pos > completion_state.start_char_index {
            crate::input::actions::update_completion_filter(
                cursor,
                &tv.rope,
                &mut completion_state,
            );
        } else if cursor.cursor_pos == completion_state.start_char_index {
            completion_state.filter.clear();
            completion_state.selected_index = 0;
        } else {
            completion_state.visible = false;
            completion_state.filter.clear();
        }
    }

    // (3) Text changed → notify the LSP server. We compare versions rather
    //     than tracking per-action flags so paste/cut/insert/etc. all flow
    //     through this single check.
    if tv.content_version != snapshot.pre_content_version {
        crate::input::actions::send_did_change(&tv.rope, lsp_client, lsp_document.as_deref_mut());
    }
}

/// Drain any unhandled action-event queues so they don't accumulate when LSP
/// is enabled and the popup intercepted the original dispatch. This is a
/// no-op in practice (handlers always drain their own events), but keeps
/// the message buffers tidy.
#[allow(dead_code)]
pub fn _drain_unused(_: MessageReader<DeleteBackwardRequested>) {}
