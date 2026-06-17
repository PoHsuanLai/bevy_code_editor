//! Document sync: flush debounced `didChange` batches and clean up timeouts.

use bevy::ecs::query::QueryData;
use bevy::prelude::*;
use bevy_instanced_text_editor::RopeBuffer;

use crate::text_view::InstancedText;
use crate::types::CodeEditor;
use bevy::input_focus::InputFocus;

use super::super::state::LspDidChangeBatcher;
use bevy_lsp::{LspClient, LspDocument};

/// Flush the [`LspDidChangeBatcher`] when its debounce timer expires.
///
/// `listen_text_edit_events` queues incremental change events and arms
/// the timer; this system ticks the timer and, on expiry, sends one
/// `textDocument/didChange` carrying the whole batch (or a full-document
/// sync if any queued edit lacked a pre-edit rope snapshot or
/// `LspConfig::full_document_sync` is on).
/// Editor state read/mutated by [`sync_lsp_document`]: the buffer to sync,
/// the optional mutable document the change batch is pushed onto, and the
/// mutable debounce batcher.
#[derive(QueryData)]
#[query_data(mutable)]
pub struct LspDocumentSyncRow {
    buffer: &'static InstancedText<RopeBuffer>,
    lsp_document: Option<&'static mut LspDocument>,
    batcher: &'static mut LspDidChangeBatcher,
}

pub fn sync_lsp_document(
    time: Res<Time>,
    mut query: Query<LspDocumentSyncRow, With<CodeEditor>>,
    input_focus: Res<InputFocus>,
) {
    let Some(focused) = input_focus.get() else {
        return;
    };
    let Ok(row) = query.get_mut(focused) else {
        return;
    };
    let LspDocumentSyncRowItem {
        buffer,
        lsp_document,
        mut batcher,
    } = row;
    if batcher.pending.is_empty() && !batcher.force_full_doc {
        return;
    }
    let Some(mut lsp_document) = lsp_document else {
        batcher.pending.clear();
        batcher.force_full_doc = false;
        return;
    };

    batcher.timer.tick(time.delta());
    if !batcher.timer.is_finished() {
        return;
    }

    if batcher.force_full_doc {
        batcher.pending.clear();
        lsp_document.push_full_sync(buffer.chunks().collect());
    } else {
        for change in std::mem::take(&mut batcher.pending) {
            lsp_document.push_change(change);
        }
    }

    batcher.force_full_doc = false;
    batcher.timer.reset();
}

/// System to clean up LSP timeout requests
pub fn cleanup_lsp_timeouts(
    sessions: Query<&crate::lsp_ui::session::LspSession, With<CodeEditor>>,
    clients: Query<&LspClient>,
) {
    for session in sessions.iter() {
        if let Ok(lsp_client) = clients.get(session.0) {
            lsp_client.cleanup_timeouts();
        }
    }
}
