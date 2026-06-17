//! Completion: drain completion + resolved-completion responses into popup state.

use bevy::ecs::query::QueryData;
use bevy::prelude::*;
use bevy_instanced_text_editor::RopeBuffer;

use crate::text_view::InstancedText;
use crate::types::{CodeEditor, CursorState};

use super::super::completion::LspCompletionPopup;
use super::super::state::CompletionLifecycle;
use bevy_lsp::{LspCompletionResponse, LspResolvedCompletionItem};

/// Editor state mutated by [`on_lsp_completion`]: cursor + buffer for
/// prefix checking, plus the mutable completion popup state / lifecycle.
#[derive(QueryData)]
#[query_data(mutable)]
pub struct CompletionResponseRow {
    cursor_state: &'static CursorState,
    buffer: &'static InstancedText<RopeBuffer>,
    completion_state: &'static mut LspCompletionPopup,
    completion_lc: &'static mut CompletionLifecycle,
    suggest: Option<&'static crate::settings::Suggest>,
}

/// Drop stale completion responses, reset resolve cache, decide visibility
/// based on whether the cursor is still in the prefix word.
pub fn on_lsp_completion(
    mut events: MessageReader<LspCompletionResponse>,
    mut q: Query<CompletionResponseRow, With<CodeEditor>>,
) {
    for ev in events.read() {
        let Ok(row) = q.get_mut(ev.entity) else {
            continue;
        };
        let CompletionResponseRowItem {
            cursor_state,
            buffer,
            mut completion_state,
            mut completion_lc,
            suggest,
        } = row;
        trace!(
            "[LSP] Completion(id={}): {} items, incomplete={}",
            ev.id,
            ev.items.len(),
            ev.is_incomplete
        );
        if !completion_lc.accept_response(ev.id) {
            continue;
        }
        let cursor_in_prefix = {
            let pos = cursor_state.cursor_pos;
            let start = completion_state.start_char_index;
            let max_prefix_len = buffer.len_chars().saturating_sub(start);
            let end_max = start + max_prefix_len;
            if pos < start || pos > end_max {
                false
            } else {
                let slice: String = buffer.slice(start..pos).chars().collect();
                slice.chars().all(|c| c.is_alphanumeric() || c == '_')
            }
        };
        completion_state.items = ev.items.clone();
        completion_state.is_incomplete = ev.is_incomplete;
        completion_state.visible = cursor_in_prefix && !completion_state.items.is_empty();
        let mode = suggest
            .map(|s| s.selection_mode)
            .unwrap_or(crate::settings::SuggestSelection::First);
        let filtered = completion_state.filtered_items();
        completion_state.selected_index = completion_state
            .preselect_index(&filtered, mode)
            .unwrap_or(0);
        // New item list invalidates any cached resolves keyed by labels that
        // may no longer be present.
        completion_state.resolved.clear();
        completion_state.pending_resolve = None;
        completion_state.resolve_request_id = completion_state.resolve_request_id.wrapping_add(1);
    }
}

pub fn on_lsp_resolved_completion(
    mut events: MessageReader<LspResolvedCompletionItem>,
    mut q: Query<&mut LspCompletionPopup, With<CodeEditor>>,
) {
    for ev in events.read() {
        let Ok(mut completion_state) = q.get_mut(ev.entity) else {
            continue;
        };
        trace!(
            "[LSP] ResolvedCompletionItem(id={}, label={})",
            ev.id,
            ev.item.label
        );
        if ev.id != completion_state.resolve_request_id {
            continue;
        }
        if let Some((label, pending_id)) = &completion_state.pending_resolve {
            if *pending_id == ev.id && label == &ev.item.label {
                completion_state
                    .resolved
                    .insert(ev.item.label.clone(), ev.item.clone());
                completion_state.pending_resolve = None;
            }
        }
    }
}
