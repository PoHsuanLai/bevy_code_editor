//! Bevy systems for LSP integration.
//!
//! Drains [`bevy_lsp::LspResponse`]s into per-editor state Components, drives
//! debounced `did_change` notifications, and emits editor-side messages
//! (navigate, multiple-locations, workspace-edit) when the response flow
//! requires host action.

use bevy::prelude::*;
use lsp_types::*;

use bevy_text_engine::FontConfig;
use crate::text_view::{ScrollState, TextBuffer, TextViewViewport};
use crate::types::{CodeEditor, CursorState};

use super::state::{
    LspCodeActionsPopup, LspCompletionPopup, LspDocumentHighlights, LspHoverPopup, LspInlayHints,
    LspDidChangeBatcher, LspRenamePopup, LspSignatureHelpPopup,
};
use bevy_lsp::{
    CodeActionOrCommand, LspClient, LspCodeActionsResponse, LspCompletionResponse,
    LspDefinitionResponse, LspDiagnosticsUpdated, LspDocument, LspDocumentHighlightsResponse,
    LspFormatResponse, LspHoverResponse, LspInlayHintsResponse, LspMessage,
    LspPrepareRenameResponse, LspReferencesResponse, LspRenameResponse,
    LspResolvedCompletionItem, LspServerCrashed, LspServerInitialized, LspShutdownAck,
    LspSignatureHelpResponse, ServerCapabilities,
};

/// Diagnostic marker for rendering in editor
#[derive(Component, Clone, Debug)]
pub struct DiagnosticMarker {
    /// Line number (0-indexed)
    pub line: usize,
    /// Diagnostic severity
    pub severity: DiagnosticSeverity,
    /// Diagnostic message
    pub message: String,
    /// Text range
    pub range: Range,
}

/// Message emitted when navigation to a different file is requested
#[derive(bevy::prelude::Message, Clone, Debug)]
pub struct NavigateToFileEvent {
    /// URI of the file to open
    pub uri: Url,
    /// Line number (0-indexed)
    pub line: usize,
    /// Character position in line (0-indexed)
    pub character: usize,
}

/// Message emitted when there are multiple definition/reference locations
#[derive(bevy::prelude::Message, Clone, Debug)]
pub struct MultipleLocationsEvent {
    /// All available locations
    pub locations: Vec<Location>,
    /// Type of locations (definition, references, etc.)
    pub location_type: LocationType,
}

/// Type of location event
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocationType {
    Definition,
    References,
}

/// Message emitted when a workspace edit needs to be applied
#[derive(bevy::prelude::Message, Clone, Debug)]
pub struct WorkspaceEditEvent {
    /// The workspace edit to apply
    pub edit: WorkspaceEdit,
}

/// Records server capabilities on the editor when the server finishes
/// `initialize`. `LspClient.initialized` is already flipped by `bevy_lsp`'s
/// drain.
pub fn on_lsp_initialized(
    mut events: MessageReader<LspServerInitialized>,
    mut q: Query<&mut ServerCapabilities, With<CodeEditor>>,
) {
    for ev in events.read() {
        let Ok(mut capabilities) = q.get_mut(ev.entity) else {
            continue;
        };
        capabilities.set(ev.capabilities.clone());
        #[cfg(debug_assertions)]
        debug!("[LSP] Server initialized");
    }
}

/// Replace `DiagnosticMarker` entities for the editor whenever the server
/// publishes a fresh diagnostic batch.
pub fn on_lsp_diagnostics(
    mut commands: Commands,
    mut events: MessageReader<LspDiagnosticsUpdated>,
    diagnostics_q: Query<Entity, With<DiagnosticMarker>>,
    editors: Query<Entity, With<CodeEditor>>,
) {
    for ev in events.read() {
        if editors.get(ev.entity).is_err() {
            continue;
        }
        // Clear old diagnostics. Silenced because another sync system or
        // hierarchy cleanup may have already despawned these markers in the
        // same tick.
        for entity in diagnostics_q.iter() {
            commands
                .entity(entity)
                .queue_silenced(bevy::ecs::system::entity_command::despawn());
        }
        for diagnostic in &ev.diagnostics {
            commands.spawn(DiagnosticMarker {
                line: diagnostic.range.start.line as usize,
                severity: diagnostic.severity.unwrap_or(DiagnosticSeverity::HINT),
                message: diagnostic.message.clone(),
                range: diagnostic.range,
            });
        }
    }
}

/// Drop stale completion responses, reset resolve cache, decide visibility
/// based on whether the cursor is still in the prefix word.
pub fn on_lsp_completion(
    mut events: MessageReader<LspCompletionResponse>,
    mut q: Query<(&CursorState, &TextBuffer, &mut LspCompletionPopup), With<CodeEditor>>,
) {
    for ev in events.read() {
        let Ok((cursor_state, buffer, mut completion_state)) = q.get_mut(ev.entity) else {
            continue;
        };
        trace!(
            "[LSP] Completion(id={}): {} items, incomplete={}",
            ev.id,
            ev.items.len(),
            ev.is_incomplete
        );
        if ev.id != completion_state.request_id {
            continue;
        }
        let cursor_in_prefix = {
            let pos = cursor_state.cursor_pos;
            let start = completion_state.start_char_index;
            let max_prefix_len = buffer.rope.len_chars().saturating_sub(start);
            let end_max = start + max_prefix_len;
            if pos < start || pos > end_max {
                false
            } else {
                let slice: String = buffer.rope.slice(start..pos).chars().collect();
                slice.chars().all(|c| c.is_alphanumeric() || c == '_')
            }
        };
        completion_state.items = ev.items.clone();
        completion_state.is_incomplete = ev.is_incomplete;
        completion_state.visible = cursor_in_prefix && !completion_state.items.is_empty();
        completion_state.selected_index = 0;
        // New item list invalidates any cached resolves keyed by labels that
        // may no longer be present.
        completion_state.resolved.clear();
        completion_state.pending_resolve = None;
        completion_state.resolve_request_id =
            completion_state.resolve_request_id.wrapping_add(1);
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

pub fn on_lsp_hover(
    mut events: MessageReader<LspHoverResponse>,
    mut q: Query<&mut LspHoverPopup, With<CodeEditor>>,
) {
    for ev in events.read() {
        let Ok(mut hover_state) = q.get_mut(ev.entity) else {
            continue;
        };
        #[cfg(debug_assertions)]
        debug!("[LSP] Hover: {} chars ({:?})", ev.content.len(), ev.kind);

        if !ev.content.is_empty() {
            if let Some(pending_pos) = hover_state.pending_char_index {
                if pending_pos == hover_state.trigger_char_index {
                    hover_state.content = ev.content.clone();
                    hover_state.kind = ev.kind.clone();
                    hover_state.range = ev.range;
                    hover_state.visible = true;
                }
            }
        }
        hover_state.pending_char_index = None;
    }
}

pub fn on_lsp_definition(
    mut events: MessageReader<LspDefinitionResponse>,
    mut q: Query<(&mut CursorState, &TextBuffer, Option<&LspDocument>), With<CodeEditor>>,
    mut navigate_events: MessageWriter<NavigateToFileEvent>,
    mut multi_location_events: MessageWriter<MultipleLocationsEvent>,
) {
    for ev in events.read() {
        let Ok((mut cursor_state, buffer, lsp_document)) = q.get_mut(ev.entity) else {
            continue;
        };
        if ev.locations.is_empty() {
            continue;
        }

        #[cfg(debug_assertions)]
        debug!("[LSP] Definition: {} location(s)", ev.locations.len());

        if ev.locations.len() > 1 {
            multi_location_events.write(MultipleLocationsEvent {
                locations: ev.locations.clone(),
                location_type: LocationType::Definition,
            });
        }

        let location = &ev.locations[0];
        let current_uri = lsp_document.map(|d| &d.uri);
        let is_same_file = current_uri.is_some_and(|uri| uri == &location.uri);

        if is_same_file {
            let line_num = location.range.start.line as usize;
            let char_in_line = location.range.start.character as usize;
            if line_num < buffer.rope.len_lines() {
                let line_start_char = buffer.rope.line_to_char(line_num);
                let target_char_pos = line_start_char + char_in_line;
                cursor_state.cursor_pos = target_char_pos.min(buffer.rope.len_chars());
            }
        } else {
            navigate_events.write(NavigateToFileEvent {
                uri: location.uri.clone(),
                line: location.range.start.line as usize,
                character: location.range.start.character as usize,
            });
        }
    }
}

pub fn on_lsp_references(
    mut events: MessageReader<LspReferencesResponse>,
    editors: Query<Entity, With<CodeEditor>>,
    mut multi_location_events: MessageWriter<MultipleLocationsEvent>,
) {
    for ev in events.read() {
        if editors.get(ev.entity).is_err() {
            continue;
        }
        #[cfg(debug_assertions)]
        debug!("[LSP] References: {} location(s)", ev.locations.len());
        if !ev.locations.is_empty() {
            multi_location_events.write(MultipleLocationsEvent {
                locations: ev.locations.clone(),
                location_type: LocationType::References,
            });
        }
    }
}

pub fn on_lsp_format(
    mut events: MessageReader<LspFormatResponse>,
    q: Query<&TextBuffer, With<CodeEditor>>,
    mut replace_writer: MessageWriter<bevy_text_editor::ReplaceRangeRequested>,
) {
    for ev in events.read() {
        let Ok(buffer) = q.get(ev.entity) else {
            continue;
        };
        trace!("[LSP] Format: {} edit(s)", ev.edits.len());
        apply_text_edits(ev.entity, buffer, ev.edits.clone(), &mut replace_writer);
    }
}

pub fn on_lsp_signature_help(
    mut events: MessageReader<LspSignatureHelpResponse>,
    mut q: Query<&mut LspSignatureHelpPopup, With<CodeEditor>>,
) {
    for ev in events.read() {
        let Ok(mut sig_state) = q.get_mut(ev.entity) else {
            continue;
        };
        #[cfg(debug_assertions)]
        debug!(
            "[LSP] SignatureHelp(id={}): {} signature(s)",
            ev.id,
            ev.signatures.len()
        );
        if ev.id != sig_state.request_id {
            continue;
        }
        sig_state.signatures = ev.signatures.clone();
        sig_state.active_signature = ev.active_signature.unwrap_or(0) as usize;
        sig_state.active_parameter = ev.active_parameter.unwrap_or(0) as usize;
        sig_state.visible = !sig_state.signatures.is_empty();
    }
}

pub fn on_lsp_code_actions(
    mut events: MessageReader<LspCodeActionsResponse>,
    mut q: Query<&mut LspCodeActionsPopup, With<CodeEditor>>,
) {
    for ev in events.read() {
        let Ok(mut action_state) = q.get_mut(ev.entity) else {
            continue;
        };
        #[cfg(debug_assertions)]
        debug!(
            "[LSP] CodeActions(id={}): {} action(s)",
            ev.id,
            ev.actions.len()
        );
        if ev.id != action_state.request_id {
            continue;
        }
        action_state.actions = ev.actions.clone();
        action_state.visible = !action_state.actions.is_empty();
        action_state.selected_index = 0;
    }
}

pub fn on_lsp_inlay_hints(
    mut events: MessageReader<LspInlayHintsResponse>,
    mut q: Query<&mut LspInlayHints, With<CodeEditor>>,
) {
    for ev in events.read() {
        let Ok(mut hint_state) = q.get_mut(ev.entity) else {
            continue;
        };
        #[cfg(debug_assertions)]
        debug!("[LSP] InlayHints: {} hint(s)", ev.hints.len());
        hint_state.hints = ev.hints.clone();
        hint_state.needs_refresh = false;
    }
}

pub fn on_lsp_document_highlights(
    mut events: MessageReader<LspDocumentHighlightsResponse>,
    mut q: Query<&mut LspDocumentHighlights, With<CodeEditor>>,
) {
    for ev in events.read() {
        let Ok(mut highlight_state) = q.get_mut(ev.entity) else {
            continue;
        };
        trace!(
            "[LSP] DocumentHighlights: {} highlight(s)",
            ev.highlights.len()
        );
        highlight_state.highlights = ev.highlights.clone();
        highlight_state.visible = !highlight_state.highlights.is_empty();
        highlight_state.in_flight_position = None;
    }
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

pub fn on_lsp_rename(
    mut events: MessageReader<LspRenameResponse>,
    mut q: Query<(&TextBuffer, Option<&LspDocument>, &mut LspRenamePopup), With<CodeEditor>>,
    mut workspace_edit_events: MessageWriter<WorkspaceEditEvent>,
    mut replace_writer: MessageWriter<bevy_text_editor::ReplaceRangeRequested>,
) {
    for ev in events.read() {
        let Ok((buffer, lsp_document, mut rename_state)) = q.get_mut(ev.entity) else {
            continue;
        };
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

pub fn on_lsp_shutdown_ack(mut events: MessageReader<LspShutdownAck>) {
    for _ev in events.read() {
        // Caller follows up with `Exit`; nothing else to do here.
        debug!("[LSP] ShutdownAck received");
    }
}

pub fn on_lsp_server_crashed(
    mut events: MessageReader<LspServerCrashed>,
    mut q: Query<
        (
            &mut LspCompletionPopup,
            &mut LspHoverPopup,
            &mut LspSignatureHelpPopup,
            &mut LspCodeActionsPopup,
            &mut LspDocumentHighlights,
            &mut LspRenamePopup,
        ),
        With<CodeEditor>,
    >,
) {
    for ev in events.read() {
        let Ok((
            mut completion_state,
            mut hover_state,
            mut sig_state,
            mut action_state,
            mut highlight_state,
            mut rename_state,
        )) = q.get_mut(ev.entity)
        else {
            continue;
        };
        warn!("[LSP] server reported crashed / channel closed");
        completion_state.dismiss();
        hover_state.reset();
        sig_state.dismiss();
        action_state.dismiss();
        highlight_state.reset();
        rename_state.reset();
    }
}

/// Apply text edits by emitting `ReplaceRangeRequested` events. The editor's
/// handler routes each through `replace_range`, keeping history, anchors,
/// and `OnEdit` consistent.
fn apply_text_edits(
    entity: Entity,
    buffer: &TextBuffer,
    edits: Vec<TextEdit>,
    writer: &mut MessageWriter<bevy_text_editor::ReplaceRangeRequested>,
) {
    let mut edits_sorted = edits;
    edits_sorted.sort_by(|a, b| {
        let a_pos = (a.range.start.line, a.range.start.character);
        let b_pos = (b.range.start.line, b.range.start.character);
        b_pos.cmp(&a_pos)
    });

    for edit in edits_sorted {
        let start_line = edit.range.start.line as usize;
        let end_line = edit.range.end.line as usize;
        let start_char_col = edit.range.start.character as usize;
        let end_char_col = edit.range.end.character as usize;

        if start_line >= buffer.rope.len_lines() {
            continue;
        }
        let start_pos =
            (buffer.rope.line_to_char(start_line) + start_char_col).min(buffer.rope.len_chars());
        let end_pos = if end_line < buffer.rope.len_lines() {
            (buffer.rope.line_to_char(end_line) + end_char_col).min(buffer.rope.len_chars())
        } else {
            buffer.rope.len_chars()
        };

        writer.write(bevy_text_editor::ReplaceRangeRequested {
            entity,
            start_char: start_pos,
            end_char: end_pos,
            text: edit.new_text,
            kind: bevy_text_editor::EditKind::Other,
            record_history: true,
        });
    }
}

/// Flush the [`LspDidChangeBatcher`] when its debounce timer expires.
///
/// `listen_text_edit_events` queues incremental change events and arms
/// the timer; this system ticks the timer and, on expiry, sends one
/// `textDocument/didChange` carrying the whole batch (or a full-document
/// sync if any queued edit lacked a pre-edit rope snapshot or
/// `LspSettings::full_document_sync` is on).
pub fn sync_lsp_document(
    time: Res<Time>,
    mut query: Query<
        (
            &TextBuffer,
            &LspClient,
            Option<&mut LspDocument>,
            &mut LspDidChangeBatcher,
        ),
        With<CodeEditor>,
    >,
) {
    let Ok((buffer, lsp_client, lsp_document, mut batcher)) = query.single_mut() else {
        return;
    };
    if batcher.pending.is_empty() && !batcher.force_full_doc {
        return;
    }
    let Some(mut lsp_document) = lsp_document else {
        // Drop queued edits — without a document URI there is no server
        // to flush to. The next edit after `LspDocument` is attached
        // re-arms the batcher cleanly.
        batcher.pending.clear();
        batcher.force_full_doc = false;
        return;
    };

    batcher.timer.tick(time.delta());
    if !batcher.timer.is_finished() {
        return;
    }

    let version = lsp_document.bump_version();
    let changes = if batcher.force_full_doc {
        batcher.pending.clear();
        vec![TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: buffer.rope.chunks().collect(),
        }]
    } else {
        std::mem::take(&mut batcher.pending)
    };

    lsp_client.send(LspMessage::DidChange {
        uri: lsp_document.uri.clone(),
        version,
        changes,
    });

    batcher.force_full_doc = false;
    batcher.timer.reset();
}

/// System to request inlay hints for visible range
pub fn request_inlay_hints(
    mut query: Query<
        (
            &LspClient,
            &ServerCapabilities,
            Ref<TextBuffer>,
            Ref<ScrollState>,
            Ref<TextViewViewport>,
            Option<&LspDocument>,
            &mut LspInlayHints,
            &FontConfig,
        ),
        With<CodeEditor>,
    >,
) {
    let Ok((lsp_client, capabilities, buffer, scroll, vp, lsp_document, mut hint_state, font)) =
        query.single_mut()
    else {
        return;
    };
    if !lsp_client.is_ready() || !capabilities.supports_inlay_hints() {
        return;
    }

    if !hint_state.needs_refresh && !buffer.is_changed() && !scroll.is_changed() && !vp.is_changed() {
        return;
    }

    let Some(lsp_document) = lsp_document else {
        return;
    };

    // Calculate visible range with some buffer
    let visible_start_line = (scroll.scroll_offset / font.line_height) as u32;
    let visible_lines = (vp.height as f32 / font.line_height) as u32 + 10;
    let visible_end_line = (visible_start_line + visible_lines).min(buffer.rope.len_lines() as u32);

    let range = Range {
        start: Position {
            line: visible_start_line,
            character: 0,
        },
        end: Position {
            line: visible_end_line,
            character: 0,
        },
    };

    // Check if range is already cached
    if hint_state.is_range_cached(&range) && !hint_state.needs_refresh {
        return;
    }

    lsp_client.send(LspMessage::InlayHint {
        uri: lsp_document.uri.clone(),
        range,
        id: 0,
    });

    hint_state.cached_range = Some(range);
    hint_state.needs_refresh = false;
}

/// System to clean up LSP timeout requests
pub fn cleanup_lsp_timeouts(query: Query<&LspClient, With<CodeEditor>>) {
    for lsp_client in query.iter() {
        lsp_client.cleanup_timeouts();
    }
}

/// Helper to send signature help request. Bumps `sig_state.request_id`
/// so the response handler can drop stale results.
pub fn request_signature_help(
    lsp_client: &LspClient,
    capabilities: &ServerCapabilities,
    uri: &Url,
    position: Position,
    sig_state: &mut LspSignatureHelpPopup,
) {
    if capabilities.supports_signature_help() {
        sig_state.request_id = sig_state.request_id.wrapping_add(1);
        lsp_client.send(LspMessage::SignatureHelp {
            uri: uri.clone(),
            position,
            id: sig_state.request_id,
        });
    }
}

/// Send `textDocument/codeAction` and bump `action_state.request_id`.
///
/// Helper, not a system — no producer wires this up yet. A future
/// "lightbulb / quick-fix" trigger system (cursor-on-diagnostic or
/// explicit `Ctrl+.`) will call this directly with the relevant range
/// and the diagnostics intersecting it.
pub fn request_code_actions(
    lsp_client: &LspClient,
    capabilities: &ServerCapabilities,
    uri: &Url,
    range: Range,
    diagnostics: Vec<Diagnostic>,
    action_state: &mut LspCodeActionsPopup,
) {
    if capabilities.supports_code_actions() {
        action_state.request_id = action_state.request_id.wrapping_add(1);
        lsp_client.send(LspMessage::CodeAction {
            uri: uri.clone(),
            range,
            diagnostics,
            id: action_state.request_id,
        });
    }
}

/// Execute a code action
pub fn execute_code_action(lsp_client: &LspClient, action: &CodeActionOrCommand) {
    match action {
        CodeActionOrCommand::Action(action) => {
            // TODO: Apply workspace edit when present.
            #[cfg(debug_assertions)]
            if let Some(edit) = &action.edit {
                debug!("[LSP] Code action has workspace edit: {:?}", edit);
            }

            // If action has command, execute it
            if let Some(command) = &action.command {
                lsp_client.send(LspMessage::ExecuteCommand {
                    command: command.command.clone(),
                    arguments: command.arguments.clone(),
                });
            }
        }
        CodeActionOrCommand::Command(command) => {
            lsp_client.send(LspMessage::ExecuteCommand {
                command: command.command.clone(),
                arguments: command.arguments.clone(),
            });
        }
    }
}

/// Fire `textDocument/documentHighlight` when the cursor settles on a
/// new position. Highlights all occurrences of the symbol under cursor
/// (the IDE feature where clicking on a name highlights every other use
/// in the same file). Debounce delay comes from
/// `LspSettings::highlight_delay_ms`.
pub fn request_document_highlights(
    time: Res<Time>,
    mut query: Query<
        (
            &LspClient,
            &ServerCapabilities,
            &CursorState,
            &TextBuffer,
            Option<&LspDocument>,
            &mut LspDocumentHighlights,
            &crate::settings::LspSettings,
        ),
        With<CodeEditor>,
    >,
) {
    let Ok((lsp_client, capabilities, cursor_state, buffer, lsp_document, mut highlight_state, settings)) =
        query.single_mut()
    else {
        return;
    };
    if !lsp_client.is_ready() || !capabilities.supports_document_highlight() {
        return;
    }

    let Some(lsp_document) = lsp_document else {
        return;
    };

    let cursor_pos = cursor_state.cursor_pos;

    if highlight_state.in_flight_position == Some(cursor_pos) {
        return;
    }
    if highlight_state.cursor_position == cursor_pos && highlight_state.visible {
        return;
    }

    if highlight_state.cursor_position != cursor_pos || highlight_state.debounce_timer.is_none() {
        highlight_state.cursor_position = cursor_pos;
        highlight_state.debounce_timer = Some(Timer::new(
            std::time::Duration::from_millis(settings.highlight_delay_ms),
            TimerMode::Once,
        ));
        if highlight_state.visible {
            highlight_state.highlights.clear();
            highlight_state.visible = false;
        }
        return;
    }

    let timer = highlight_state.debounce_timer.as_mut().unwrap();
    timer.tick(time.delta());
    if !timer.is_finished() {
        return;
    }
    highlight_state.debounce_timer = None;
    highlight_state.in_flight_position = Some(cursor_pos);

    let position =
        bevy_lsp::rope_char_to_lsp_position(&buffer.rope, cursor_pos, capabilities.position_encoding());
    lsp_client.send(LspMessage::DocumentHighlight {
        uri: lsp_document.uri.clone(),
        position,
        id: 0,
    });
}

/// Helper to request prepare rename
pub fn request_prepare_rename(
    lsp_client: &LspClient,
    capabilities: &ServerCapabilities,
    uri: &Url,
    position: Position,
) {
    if capabilities.supports_prepare_rename() {
        lsp_client.send(LspMessage::PrepareRename {
            uri: uri.clone(),
            position,
            id: 0,
        });
    }
    // If server supports rename but not prepare, the caller handles the dialog directly.
    // If server doesn't support rename at all, start_prepare was already called — reset it.
}

/// Helper to execute rename
pub fn execute_rename(
    lsp_client: &LspClient,
    capabilities: &ServerCapabilities,
    uri: &Url,
    position: Position,
    new_name: String,
) {
    if capabilities.supports_rename() {
        lsp_client.send(LspMessage::Rename {
            uri: uri.clone(),
            position,
            new_name,
            id: 0,
        });
    }
}
