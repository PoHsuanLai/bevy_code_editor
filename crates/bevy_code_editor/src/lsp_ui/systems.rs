//! Bevy systems for LSP integration.
//!
//! Drains [`bevy_lsp::LspResponse`]s into per-editor state Components, drives
//! debounced `did_change` notifications, and emits editor-side messages
//! (navigate, multiple-locations, workspace-edit) when the response flow
//! requires host action.

use bevy::prelude::*;
use lsp_types::*;

use bevy_text_engine::FontConfig;
use crate::text_view::{TextViewState, TextViewViewport};
use crate::types::{CodeEditor, CursorState};

use super::state::{
    LspCodeActionsPopup, LspCompletionPopup, LspDocumentHighlights, LspHoverPopup, LspInlayHints,
    LspRenamePopup, LspSignatureHelpPopup, LspSyncStateExtra,
};
use bevy_lsp::{
    CodeActionOrCommand, LspClient, LspDocument, LspMessage, LspResponse, ServerCapabilities,
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

/// System to process LSP messages and update per-editor Components.
#[allow(clippy::too_many_arguments)]
pub fn process_lsp_messages(
    mut commands: Commands,
    diagnostics_query: Query<Entity, With<DiagnosticMarker>>,
    mut editor_query: Query<
        (
            Entity,
            &mut LspClient,
            &mut ServerCapabilities,
            &mut CursorState,
            &mut TextViewState,
            Option<&LspDocument>,
            &mut LspCompletionPopup,
            &mut LspHoverPopup,
            &mut LspSignatureHelpPopup,
            &mut LspCodeActionsPopup,
            &mut LspInlayHints,
            &mut LspDocumentHighlights,
            &mut LspRenamePopup,
        ),
        With<CodeEditor>,
    >,
    mut navigate_events: MessageWriter<NavigateToFileEvent>,
    mut multi_location_events: MessageWriter<MultipleLocationsEvent>,
    mut workspace_edit_events: MessageWriter<WorkspaceEditEvent>,
    mut replace_writer: MessageWriter<bevy_text_editor::ReplaceRangeRequested>,
) {
    let Ok((
        editor_entity,
        mut lsp_client,
        mut capabilities,
        mut cursor_state,
        tv,
        lsp_document,
        mut completion_state,
        mut hover_state,
        mut sig_state,
        mut action_state,
        mut hint_state,
        mut highlight_state,
        mut rename_state,
    )) = editor_query.single_mut()
    else {
        return;
    };
    // Clean up timed out requests periodically
    lsp_client.cleanup_timeouts();

    while let Some(response) = lsp_client.try_recv() {
        match response {
            LspResponse::Initialized {
                capabilities: caps,
            } => {
                lsp_client.initialized = true;
                capabilities.set(caps);
                #[cfg(debug_assertions)]
                debug!("[LSP] Server initialized");
            }

            LspResponse::Diagnostics {
                uri: _,
                diagnostics,
            } => {
                // Clear old diagnostics. Silenced because another sync
                // system or hierarchy cleanup may have already despawned
                // these markers in the same tick.
                for entity in diagnostics_query.iter() {
                    commands.entity(entity).queue_silenced(
                        bevy::ecs::system::entity_command::despawn(),
                    );
                }

                for diagnostic in diagnostics {
                    commands.spawn(DiagnosticMarker {
                        line: diagnostic.range.start.line as usize,
                        severity: diagnostic.severity.unwrap_or(DiagnosticSeverity::HINT),
                        message: diagnostic.message.clone(),
                        range: diagnostic.range,
                    });
                }
            }

            LspResponse::Completion {
                id,
                items,
                is_incomplete,
            } => {
                trace!(
                    "[LSP] Completion(id={}): {} items, incomplete={}",
                    id,
                    items.len(),
                    is_incomplete
                );
                // Drop stale: a newer request was issued (or the popup was
                // dismissed, which also bumps `request_id`) while this
                // response was in flight.
                if id != completion_state.request_id {
                    continue;
                }
                // Drop if cursor is no longer in a position where
                // completions make sense. Mirrors Zed's `continue_showing`:
                // keep when (a) cursor at-or-after the menu's anchor and
                // (b) prefix is all word chars.
                let cursor_in_prefix = {
                    let pos = cursor_state.cursor_pos;
                    let start = completion_state.start_char_index;
                    let max_prefix_len =
                        tv.rope.len_chars().saturating_sub(start);
                    let end_max = start + max_prefix_len;
                    if pos < start || pos > end_max {
                        false
                    } else {
                        let slice: String =
                            tv.rope.slice(start..pos).chars().collect();
                        slice.chars().all(|c| c.is_alphanumeric() || c == '_')
                    }
                };
                completion_state.items = items;
                completion_state.is_incomplete = is_incomplete;
                completion_state.visible =
                    cursor_in_prefix && !completion_state.items.is_empty();
                completion_state.selected_index = 0;
                // New item list invalidates any cached resolves keyed by
                // labels that may no longer be present.
                completion_state.resolved.clear();
                completion_state.pending_resolve = None;
                completion_state.resolve_request_id =
                    completion_state.resolve_request_id.wrapping_add(1);
            }

            LspResponse::ResolvedCompletionItem { id, item } => {
                trace!(
                    "[LSP] ResolvedCompletionItem(id={}, label={})",
                    id,
                    item.label
                );
                // Drop stale: a newer resolve was requested or the popup
                // was dismissed.
                if id != completion_state.resolve_request_id {
                    continue;
                }
                if let Some((label, pending_id)) = &completion_state.pending_resolve {
                    if *pending_id == id && label == &item.label {
                        completion_state
                            .resolved
                            .insert(item.label.clone(), item);
                        completion_state.pending_resolve = None;
                    }
                }
            }

            LspResponse::Hover { content, range } => {
                #[cfg(debug_assertions)]
                debug!("[LSP] Hover: {} chars", content.len());

                if !content.is_empty() {
                    if let Some(pending_pos) = hover_state.pending_char_index {
                        if pending_pos == hover_state.trigger_char_index {
                            hover_state.content = content;
                            hover_state.range = range;
                            hover_state.visible = true;
                        }
                    }
                }
                hover_state.pending_char_index = None;
            }

            LspResponse::Definition { locations } => {
                if locations.is_empty() {
                    continue;
                }

                #[cfg(debug_assertions)]
                debug!("[LSP] Definition: {} location(s)", locations.len());

                if locations.len() > 1 {
                    multi_location_events.write(MultipleLocationsEvent {
                        locations: locations.clone(),
                        location_type: LocationType::Definition,
                    });
                }

                let location = &locations[0];
                let current_uri = lsp_document.map(|d| &d.uri);
                let is_same_file = current_uri.is_some_and(|uri| uri == &location.uri);

                if is_same_file {
                    let line_num = location.range.start.line as usize;
                    let char_in_line = location.range.start.character as usize;

                    if line_num < tv.rope.len_lines() {
                        let line_start_char = tv.rope.line_to_char(line_num);
                        let target_char_pos = line_start_char + char_in_line;
                        cursor_state.cursor_pos = target_char_pos.min(tv.rope.len_chars());
                    }
                } else {
                    navigate_events.write(NavigateToFileEvent {
                        uri: location.uri.clone(),
                        line: location.range.start.line as usize,
                        character: location.range.start.character as usize,
                    });
                }
            }

            LspResponse::References { locations } => {
                #[cfg(debug_assertions)]
                debug!("[LSP] References: {} location(s)", locations.len());

                if !locations.is_empty() {
                    multi_location_events.write(MultipleLocationsEvent {
                        locations,
                        location_type: LocationType::References,
                    });
                }
            }

            LspResponse::Format { edits } => {
                trace!("[LSP] Format: {} edit(s)", edits.len());
                apply_text_edits(editor_entity, &tv, edits, &mut replace_writer);
            }

            LspResponse::SignatureHelp {
                id,
                signatures,
                active_signature,
                active_parameter,
            } => {
                #[cfg(debug_assertions)]
                debug!("[LSP] SignatureHelp(id={}): {} signature(s)", id, signatures.len());
                if id != sig_state.request_id {
                    continue;
                }
                sig_state.signatures = signatures;
                sig_state.active_signature = active_signature.unwrap_or(0) as usize;
                sig_state.active_parameter = active_parameter.unwrap_or(0) as usize;
                sig_state.visible = !sig_state.signatures.is_empty();
            }

            LspResponse::CodeActions { id, actions } => {
                #[cfg(debug_assertions)]
                debug!("[LSP] CodeActions(id={}): {} action(s)", id, actions.len());
                if id != action_state.request_id {
                    continue;
                }
                action_state.actions = actions;
                action_state.visible = !action_state.actions.is_empty();
                action_state.selected_index = 0;
            }

            LspResponse::InlayHints { hints } => {
                #[cfg(debug_assertions)]
                debug!("[LSP] InlayHints: {} hint(s)", hints.len());

                hint_state.hints = hints;
                hint_state.needs_refresh = false;
            }

            LspResponse::DocumentHighlights { highlights } => {
                trace!(
                    "[LSP] DocumentHighlights: {} highlight(s)",
                    highlights.len()
                );

                highlight_state.highlights = highlights;
                highlight_state.visible = !highlight_state.highlights.is_empty();
                highlight_state.in_flight_position = None;
            }

            LspResponse::PrepareRename { range, placeholder } => {
                trace!(
                    "[LSP] PrepareRename: range={:?}, placeholder={:?}",
                    range,
                    placeholder
                );

                rename_state.on_prepare_response(range, placeholder);
            }

            LspResponse::Rename { edit } => {
                #[cfg(debug_assertions)]
                debug!("[LSP] Rename: workspace edit received");

                // Apply edits to current document if present
                if let Some(changes) = &edit.changes {
                    if let Some(doc) = lsp_document {
                        if let Some(edits) = changes.get(&doc.uri) {
                            apply_text_edits(editor_entity, &tv, edits.clone(), &mut replace_writer);
                        }
                    }
                }

                // Emit event for external handling (other files)
                workspace_edit_events.write(WorkspaceEditEvent { edit });

                // Close rename dialog
                rename_state.reset();
            }

            LspResponse::ShutdownAck { .. } => {
                // Caller follows up with `Exit`; nothing else to do here.
                debug!("[LSP] ShutdownAck received");
            }

            LspResponse::Crashed => {
                warn!("[LSP] server reported crashed / channel closed");
                completion_state.dismiss();
                hover_state.reset();
                sig_state.dismiss();
                action_state.dismiss();
                highlight_state.reset();
                rename_state.reset();
            }
        }
    }
}

/// Apply text edits by emitting `ReplaceRangeRequested` events. The editor's
/// handler routes each through `replace_range`, keeping history, anchors,
/// and `OnEdit` consistent.
fn apply_text_edits(
    entity: Entity,
    tv: &TextViewState,
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

        if start_line >= tv.rope.len_lines() {
            continue;
        }
        let start_pos =
            (tv.rope.line_to_char(start_line) + start_char_col).min(tv.rope.len_chars());
        let end_pos = if end_line < tv.rope.len_lines() {
            (tv.rope.line_to_char(end_line) + end_char_col).min(tv.rope.len_chars())
        } else {
            tv.rope.len_chars()
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

/// System to sync document with LSP (debounced).
///
/// Drives `did_change` from `LspSyncStateExtra::dirty` (set by editor input
/// when text changes); the LSP-side version counter lives on `LspDocument`.
pub fn sync_lsp_document(
    time: Res<Time>,
    mut query: Query<
        (
            &TextViewState,
            &LspClient,
            Option<&mut LspDocument>,
            &mut LspSyncStateExtra,
        ),
        With<CodeEditor>,
    >,
) {
    let Ok((tv, lsp_client, lsp_document, mut sync_state)) = query.single_mut() else {
        return;
    };
    if !sync_state.dirty {
        return;
    }
    let Some(mut lsp_document) = lsp_document else {
        return;
    };

    sync_state.timer.tick(time.delta());

    if sync_state.timer.is_finished() {
        let version = lsp_document.bump_version();

        // OPTIMIZATION: Use rope chunks instead of full to_string() conversion
        let change = TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: tv.rope.chunks().collect(),
        };

        lsp_client.send(LspMessage::DidChange {
            uri: lsp_document.uri.clone(),
            version,
            changes: vec![change],
        });

        #[cfg(debug_assertions)]
        debug!("[LSP] Debounced sync sent, version={}", version);

        sync_state.dirty = false;
        sync_state.timer.reset();
    }
}

/// System to request inlay hints for visible range
pub fn request_inlay_hints(
    mut query: Query<
        (
            &LspClient,
            &ServerCapabilities,
            Ref<TextViewState>,
            Ref<TextViewViewport>,
            Option<&LspDocument>,
            &mut LspInlayHints,
            &FontConfig,
        ),
        With<CodeEditor>,
    >,
) {
    let Ok((lsp_client, capabilities, tv, vp, lsp_document, mut hint_state, font)) =
        query.single_mut()
    else {
        return;
    };
    if !lsp_client.is_ready() || !capabilities.supports_inlay_hints() {
        return;
    }

    if !hint_state.needs_refresh && !tv.is_changed() && !vp.is_changed() {
        return;
    }

    let Some(lsp_document) = lsp_document else {
        return;
    };

    // Calculate visible range with some buffer
    let visible_start_line = (tv.scroll_offset / font.line_height) as u32;
    let visible_lines = (vp.height as f32 / font.line_height) as u32 + 10;
    let visible_end_line = (visible_start_line + visible_lines).min(tv.rope.len_lines() as u32);

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

/// Helper to send code action request. Bumps `action_state.request_id`.
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
            // If action has edit, apply it directly
            if let Some(edit) = &action.edit {
                // TODO: Apply workspace edit
                #[cfg(debug_assertions)]
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

/// System to request document highlights when cursor moves
pub fn request_document_highlights(
    time: Res<Time>,
    mut query: Query<
        (
            &LspClient,
            &ServerCapabilities,
            &CursorState,
            &TextViewState,
            Option<&LspDocument>,
            &mut LspDocumentHighlights,
        ),
        With<CodeEditor>,
    >,
) {
    let Ok((lsp_client, capabilities, cursor_state, tv, lsp_document, mut highlight_state)) =
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
        highlight_state.debounce_timer = Some(Timer::from_seconds(0.15, TimerMode::Once));
        // Cursor moved: hide stale highlights immediately so they don't sit on
        // the old symbol while we wait for the new response.
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

    let position = bevy_lsp::rope_char_to_lsp_position(
        &tv.rope,
        cursor_pos,
        bevy_lsp::PositionEncoding::Utf16,
    );
    lsp_client.send(LspMessage::DocumentHighlight {
        uri: lsp_document.uri.clone(),
        position,
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
        });
    }
}
