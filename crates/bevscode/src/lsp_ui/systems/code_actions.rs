//! Code actions: drain responses, request, and execute code actions.

use bevy::prelude::*;
use lsp_types::*;

use crate::types::CodeEditor;

use super::super::state::{CodeActionsLifecycle, LspCodeActionsPopup};
use bevy_lsp::{
    CodeActionOrCommand, LspCodeActionsResponse, LspMessage, LspRequest, ServerCapabilities,
};

pub fn on_lsp_code_actions(
    mut events: MessageReader<LspCodeActionsResponse>,
    mut q: Query<(&mut LspCodeActionsPopup, &mut CodeActionsLifecycle), With<CodeEditor>>,
) {
    for ev in events.read() {
        let Ok((mut action_state, mut action_lc)) = q.get_mut(ev.entity) else {
            continue;
        };
        #[cfg(debug_assertions)]
        debug!(
            "[LSP] CodeActions(id={}): {} action(s)",
            ev.id,
            ev.actions.len()
        );
        if !action_lc.accept_response(ev.id) {
            continue;
        }
        action_state.actions = ev.actions.clone();
        action_state.visible = !action_state.actions.is_empty();
        action_state.selected_index = 0;
    }
}

/// Send `textDocument/codeAction`. The id-bump lives on
/// [`CodeActionsLifecycle`] so the response handler can drop stale
/// results.
///
/// Helper, not a system — no producer wires this up yet. A future
/// "lightbulb / quick-fix" trigger system (cursor-on-diagnostic or
/// explicit `Ctrl+.`) will call this directly with the relevant range
/// and the diagnostics intersecting it.
pub fn request_code_actions(
    entity: Entity,
    capabilities: &ServerCapabilities,
    uri: &Url,
    range: Range,
    diagnostics: Vec<Diagnostic>,
    action_lc: &mut CodeActionsLifecycle,
    lsp_w: &mut MessageWriter<LspRequest>,
) {
    if capabilities.supports_code_actions() {
        let id = action_lc.new_request();
        lsp_w.write(LspRequest {
            entity,
            origin: None,
            msg: LspMessage::CodeAction {
                uri: uri.clone(),
                range,
                diagnostics,
                id,
            },
        });
    }
}

/// Execute a code action
pub fn execute_code_action(
    entity: Entity,
    action: &CodeActionOrCommand,
    lsp_w: &mut MessageWriter<LspRequest>,
) {
    match action {
        CodeActionOrCommand::Action(action) => {
            #[cfg(debug_assertions)]
            if let Some(edit) = &action.edit {
                debug!("[LSP] Code action has workspace edit: {:?}", edit);
            }

            if let Some(command) = &action.command {
                lsp_w.write(LspRequest {
                    entity,
                    origin: None,
                    msg: LspMessage::ExecuteCommand {
                        command: command.command.clone(),
                        arguments: command.arguments.clone(),
                    },
                });
            }
        }
        CodeActionOrCommand::Command(command) => {
            lsp_w.write(LspRequest {
                entity,
                origin: None,
                msg: LspMessage::ExecuteCommand {
                    command: command.command.clone(),
                    arguments: command.arguments.clone(),
                },
            });
        }
    }
}
