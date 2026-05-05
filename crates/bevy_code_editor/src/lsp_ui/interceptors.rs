//! Per-popup `EditorAction` interceptors.
//!
//! Hosted by the LSP UI feature so the dispatcher (`crate::input::dispatch`)
//! doesn't need to know about popup state details — each interceptor
//! returns `true` when it consumed the action and the dispatcher early-
//! returns. The editing handlers in `bevy_text_editor` never see the
//! consumed event.

use crate::input::actions;
use crate::input::keybindings::EditorAction;
use crate::lsp_ui::state::LspCompletionPopup;
use crate::settings::LspSettings;
use crate::text_view::TextViewState;
use crate::types::{CodeEditor, CursorState};
use bevy::ecs::world::Mut;
use bevy::prelude::*;

/// LSP completion popup interceptor.
///
/// When the popup is visible on `focused` and has filtered items, certain
/// `EditorAction`s are reinterpreted as popup navigation:
/// - `MoveCursorUp` / `MoveCursorDown`: cycle the selected item.
/// - `InsertNewline` / `InsertTab`: apply the selected item via
///   [`actions::apply_completion`] and emit `did_change`.
/// - `ClearSelection`: dismiss the popup.
///
/// Returns `true` when the action was consumed; the caller (dispatcher)
/// short-circuits without emitting any `*Requested` event.
pub fn completion_popup_intercept(
    action: EditorAction,
    focused: Entity,
    completion_state: &mut Mut<'_, LspCompletionPopup>,
    lsp_client: &bevy_lsp::LspClient,
    lsp_document: Option<&mut bevy_lsp::LspDocument>,
    editor_q: &mut Query<
        (
            &mut CursorState,
            &mut TextViewState,
            &mut crate::types::fold::GotoLineState,
        ),
        With<CodeEditor>,
    >,
    lsp_settings: &LspSettings,
    replace_writer: &mut MessageWriter<bevy_text_editor::ReplaceRangeRequested>,
) -> bool {
    let filtered_count = completion_state.filtered_items().len();
    let max_visible = lsp_settings.completion.max_items;
    if !completion_state.visible || filtered_count == 0 {
        return false;
    }
    match action {
        EditorAction::MoveCursorUp => {
            if completion_state.selected_index > 0 {
                completion_state.selected_index -= 1;
            } else {
                completion_state.selected_index = filtered_count.saturating_sub(1);
            }
            completion_state.ensure_selected_visible_with_max(max_visible);
            true
        }
        EditorAction::MoveCursorDown => {
            if completion_state.selected_index + 1 < filtered_count {
                completion_state.selected_index += 1;
            } else {
                completion_state.selected_index = 0;
            }
            completion_state.ensure_selected_visible_with_max(max_visible);
            true
        }
        EditorAction::InsertNewline | EditorAction::InsertTab => {
            if let Ok((cursor, tv, _)) = editor_q.get(focused) {
                actions::apply_completion(
                    focused,
                    cursor.cursor_pos,
                    completion_state,
                    replace_writer,
                );
                actions::send_did_change(&tv.rope, lsp_client, lsp_document);
            }
            true
        }
        EditorAction::ClearSelection => {
            completion_state.visible = false;
            completion_state.filter.clear();
            completion_state.scroll_offset = 0;
            true
        }
        _ => false,
    }
}
