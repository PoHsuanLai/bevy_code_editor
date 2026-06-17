//! Rename: prepare/rename responses, request helpers, and workspace-edit event.

use bevy::ecs::query::QueryData;
use bevy::prelude::*;
use bevy_instanced_text_editor::RopeBuffer;
use lsp_types::*;

use crate::text_view::InstancedText;
use crate::types::CodeEditor;

use super::super::state::LspRenamePopup;
use super::shared::apply_text_edits;
use bevy_lsp::{
    LspDocument, LspMessage, LspPrepareRenameResponse, LspRenameResponse, LspRequest,
    ServerCapabilities,
};

/// Message emitted when a workspace edit needs to be applied
#[derive(bevy::prelude::Message, Clone, Debug)]
pub struct WorkspaceEditEvent {
    /// The workspace edit to apply
    pub edit: WorkspaceEdit,
}

pub fn on_lsp_prepare_rename(
    mut events: MessageReader<LspPrepareRenameResponse>,
    mut q: Query<&mut LspRenamePopup, With<CodeEditor>>,
) {
    for ev in events.read() {
        let Ok(mut rename_state) = q.get_mut(ev.entity) else {
            continue;
        };
        trace!(
            "[LSP] PrepareRename: range={:?}, placeholder={:?}",
            ev.range,
            ev.placeholder
        );
        rename_state.on_prepare_response(ev.range, ev.placeholder.clone());
    }
}

/// Editor state read/mutated by [`on_lsp_rename`]: the buffer and document
/// to apply the workspace edit against, plus the mutable rename popup state.
#[derive(QueryData)]
#[query_data(mutable)]
pub struct RenameResponseRow {
    buffer: &'static InstancedText<RopeBuffer>,
    lsp_document: Option<&'static LspDocument>,
    rename_state: &'static mut LspRenamePopup,
}

pub fn on_lsp_rename(
    mut events: MessageReader<LspRenameResponse>,
    mut q: Query<RenameResponseRow, With<CodeEditor>>,
    mut workspace_edit_events: MessageWriter<WorkspaceEditEvent>,
    mut replace_writer: MessageWriter<bevy_instanced_text_editor::ReplaceRangeRequested>,
) {
    for ev in events.read() {
        let Ok(row) = q.get_mut(ev.entity) else {
            continue;
        };
        let RenameResponseRowItem {
            buffer,
            lsp_document,
            mut rename_state,
        } = row;
        #[cfg(debug_assertions)]
        debug!("[LSP] Rename: workspace edit received");

        if let Some(changes) = &ev.edit.changes {
            if let Some(doc) = lsp_document {
                if let Some(edits) = changes.get(&doc.uri) {
                    apply_text_edits(ev.entity, buffer, edits.clone(), &mut replace_writer);
                }
            }
        }

        workspace_edit_events.write(WorkspaceEditEvent {
            edit: ev.edit.clone(),
        });
        rename_state.reset();
    }
}

/// Helper to request prepare rename
pub fn request_prepare_rename(
    entity: Entity,
    capabilities: &ServerCapabilities,
    uri: &Url,
    position: Position,
    lsp_w: &mut MessageWriter<LspRequest>,
) {
    if capabilities.supports_prepare_rename() {
        lsp_w.write(LspRequest {
            entity,
            origin: None,
            msg: LspMessage::PrepareRename {
                uri: uri.clone(),
                position,
                id: 0,
            },
        });
    }
}

pub fn execute_rename(
    entity: Entity,
    capabilities: &ServerCapabilities,
    uri: &Url,
    position: Position,
    new_name: String,
    lsp_w: &mut MessageWriter<LspRequest>,
) {
    if capabilities.supports_rename() {
        lsp_w.write(LspRequest {
            entity,
            origin: None,
            msg: LspMessage::Rename {
                uri: uri.clone(),
                position,
                new_name,
                id: 0,
            },
        });
    }
}
