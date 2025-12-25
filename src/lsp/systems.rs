//! Bevy systems for LSP integration

use bevy::prelude::*;
use lsp_types::*;

use crate::settings::*;
use crate::types::CodeEditorState;

use super::client::LspClient;
use super::messages::{CodeActionOrCommand, LspMessage, LspResponse};
use super::state::{
    CodeActionState, CompletionState, DocumentHighlightState, HoverState, InlayHintState,
    LspSyncState, RenameState, SignatureHelpState,
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

/// System to process LSP messages and update Bevy resources
pub fn process_lsp_messages(
    mut lsp_client: ResMut<LspClient>,
    mut commands: Commands,
    diagnostics_query: Query<Entity, With<DiagnosticMarker>>,
    mut completion_state: ResMut<CompletionState>,
    mut hover_state: ResMut<HoverState>,
    mut sig_state: ResMut<SignatureHelpState>,
    mut action_state: ResMut<CodeActionState>,
    mut hint_state: ResMut<InlayHintState>,
    mut highlight_state: ResMut<DocumentHighlightState>,
    mut rename_state: ResMut<RenameState>,
    mut editor_state: ResMut<CodeEditorState>,
    lsp_sync: Res<LspSyncState>,
    mut navigate_events: MessageWriter<NavigateToFileEvent>,
    mut multi_location_events: MessageWriter<MultipleLocationsEvent>,
    mut workspace_edit_events: MessageWriter<WorkspaceEditEvent>,
) {
    // Clean up timed out requests periodically
    lsp_client.cleanup_timeouts();

    while let Some(response) = lsp_client.try_recv() {
        match response {
            LspResponse::Initialized { capabilities: _ } => {
                lsp_client.initialized = true;
                #[cfg(debug_assertions)]
                eprintln!("[LSP] Server initialized");
            }

            LspResponse::Diagnostics { uri: _, diagnostics } => {
                // Clear old diagnostics
                for entity in diagnostics_query.iter() {
                    commands.entity(entity).despawn();
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

            LspResponse::Completion { items, is_incomplete } => {
                #[cfg(debug_assertions)]
                eprintln!("[LSP] Completion: {} items, incomplete={}", items.len(), is_incomplete);
                completion_state.items = items;
                completion_state.is_incomplete = is_incomplete;
                completion_state.visible = !completion_state.items.is_empty();
                completion_state.selected_index = 0;
            }

            LspResponse::Hover { content, range } => {
                #[cfg(debug_assertions)]
                eprintln!("[LSP] Hover: {} chars", content.len());

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
                eprintln!("[LSP] Definition: {} location(s)", locations.len());

                if locations.len() > 1 {
                    multi_location_events.write(MultipleLocationsEvent {
                        locations: locations.clone(),
                        location_type: LocationType::Definition,
                    });
                }

                let location = &locations[0];
                let current_uri = lsp_sync.document_uri.as_ref();
                let is_same_file = current_uri.is_some_and(|uri| uri == &location.uri);

                if is_same_file {
                    let line_num = location.range.start.line as usize;
                    let char_in_line = location.range.start.character as usize;

                    if line_num < editor_state.rope.len_lines() {
                        let line_start_char = editor_state.rope.line_to_char(line_num);
                        let target_char_pos = line_start_char + char_in_line;
                        editor_state.cursor_pos = target_char_pos.min(editor_state.rope.len_chars());
                        editor_state.needs_update = true;
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
                eprintln!("[LSP] References: {} location(s)", locations.len());

                if !locations.is_empty() {
                    multi_location_events.write(MultipleLocationsEvent {
                        locations,
                        location_type: LocationType::References,
                    });
                }
            }

            LspResponse::Format { edits } => {
                #[cfg(debug_assertions)]
                eprintln!("[LSP] Format: {} edit(s)", edits.len());
                apply_text_edits(&mut editor_state, edits);
            }

            LspResponse::SignatureHelp {
                signatures,
                active_signature,
                active_parameter,
            } => {
                #[cfg(debug_assertions)]
                eprintln!("[LSP] SignatureHelp: {} signature(s)", signatures.len());

                sig_state.signatures = signatures;
                sig_state.active_signature = active_signature.unwrap_or(0) as usize;
                sig_state.active_parameter = active_parameter.unwrap_or(0) as usize;
                sig_state.visible = !sig_state.signatures.is_empty();
            }

            LspResponse::CodeActions { actions } => {
                #[cfg(debug_assertions)]
                eprintln!("[LSP] CodeActions: {} action(s)", actions.len());

                action_state.actions = actions;
                action_state.visible = !action_state.actions.is_empty();
                action_state.selected_index = 0;
            }

            LspResponse::InlayHints { hints } => {
                #[cfg(debug_assertions)]
                eprintln!("[LSP] InlayHints: {} hint(s)", hints.len());

                hint_state.hints = hints;
                hint_state.needs_refresh = false;
            }

            LspResponse::DocumentHighlights { highlights } => {
                #[cfg(debug_assertions)]
                eprintln!("[LSP] DocumentHighlights: {} highlight(s)", highlights.len());

                highlight_state.highlights = highlights;
                highlight_state.visible = !highlight_state.highlights.is_empty();
            }

            LspResponse::PrepareRename { range, placeholder } => {
                #[cfg(debug_assertions)]
                eprintln!("[LSP] PrepareRename: range={:?}, placeholder={:?}", range, placeholder);

                rename_state.on_prepare_response(range, placeholder);
            }

            LspResponse::Rename { edit } => {
                #[cfg(debug_assertions)]
                eprintln!("[LSP] Rename: workspace edit received");

                // Apply edits to current document if present
                if let Some(changes) = &edit.changes {
                    if let Some(uri) = &lsp_sync.document_uri {
                        if let Some(edits) = changes.get(uri) {
                            apply_text_edits(&mut editor_state, edits.clone());
                        }
                    }
                }

                // Emit event for external handling (other files)
                workspace_edit_events.write(WorkspaceEditEvent { edit });

                // Close rename dialog
                rename_state.reset();
            }
        }
    }
}

/// Apply text edits from formatting
fn apply_text_edits(editor_state: &mut CodeEditorState, edits: Vec<TextEdit>) {
    // Sort edits in reverse order to preserve positions
    let mut edits_sorted = edits;
    edits_sorted.sort_by(|a, b| {
        let a_pos = (a.range.start.line, a.range.start.character);
        let b_pos = (b.range.start.line, b.range.start.character);
        b_pos.cmp(&a_pos)
    });

    for edit in edits_sorted {
        let start_line = edit.range.start.line as usize;
        let end_line = edit.range.end.line as usize;
        let start_char = edit.range.start.character as usize;
        let end_char = edit.range.end.character as usize;

        if start_line < editor_state.rope.len_lines() {
            let start_line_char = editor_state.rope.line_to_char(start_line);
            let start_pos = start_line_char + start_char;

            let end_pos = if end_line < editor_state.rope.len_lines() {
                let end_line_char = editor_state.rope.line_to_char(end_line);
                (end_line_char + end_char).min(editor_state.rope.len_chars())
            } else {
                editor_state.rope.len_chars()
            };

            let start_pos = start_pos.min(editor_state.rope.len_chars());
            let end_pos = end_pos.min(editor_state.rope.len_chars());

            if start_pos <= end_pos {
                let start_byte = editor_state.rope.char_to_byte(start_pos);
                let end_byte = editor_state.rope.char_to_byte(end_pos);
                let new_end_byte = start_byte + edit.new_text.len();

                #[cfg(feature = "tree-sitter")]
                editor_state.record_edit(start_byte, end_byte, new_end_byte);

                editor_state.rope.remove(start_byte..end_byte);
                editor_state.rope.insert(start_pos, &edit.new_text);
            }
        }
    }

    editor_state.needs_update = true;
    editor_state.pending_update = false;
    editor_state.content_version += 1;
    editor_state.dirty_lines = None;
    editor_state.previous_line_count = editor_state.rope.len_lines();
}

/// System to sync document with LSP (debounced)
pub fn sync_lsp_document(
    time: Res<Time>,
    mut sync_state: ResMut<LspSyncState>,
    editor_state: Res<CodeEditorState>,
    lsp_client: Res<LspClient>,
) {
    if !sync_state.dirty {
        return;
    }

    sync_state.timer.tick(time.delta());

    if sync_state.timer.is_finished() {
        if let Some(uri) = &sync_state.document_uri {
            let version = sync_state.document_version;

            // OPTIMIZATION: Use rope chunks instead of full to_string() conversion
            let change = TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: editor_state.rope.chunks().collect(),
            };

            lsp_client.send(LspMessage::DidChange {
                uri: uri.clone(),
                version,
                changes: vec![change],
            });

            #[cfg(debug_assertions)]
            eprintln!("[LSP] Debounced sync sent, version={}", version);
        }

        sync_state.dirty = false;
        sync_state.timer.reset();
    }
}

/// System to request inlay hints for visible range
pub fn request_inlay_hints(
    lsp_client: Res<LspClient>,
    editor_state: Res<CodeEditorState>,
    lsp_sync: Res<LspSyncState>,
    mut hint_state: ResMut<InlayHintState>,
    settings: Res<EditorSettings>,
    viewport: Res<crate::types::ViewportDimensions>,
) {
    if !lsp_client.is_ready() || !lsp_client.capabilities.supports_inlay_hints() {
        return;
    }

    if !hint_state.needs_refresh && !editor_state.is_changed() && !viewport.is_changed() {
        return;
    }

    let Some(uri) = &lsp_sync.document_uri else {
        return;
    };

    // Calculate visible range with some buffer
    let visible_start_line = (editor_state.scroll_offset / settings.font.line_height) as u32;
    let visible_lines = (viewport.height as f32 / settings.font.line_height) as u32 + 10;
    let visible_end_line = (visible_start_line + visible_lines).min(editor_state.rope.len_lines() as u32);

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
        uri: uri.clone(),
        range: range.clone(),
    });

    hint_state.cached_range = Some(range);
    hint_state.needs_refresh = false;
}

/// System to clean up LSP timeout requests
pub fn cleanup_lsp_timeouts(lsp_client: Res<LspClient>) {
    lsp_client.cleanup_timeouts();
}

/// Helper to send signature help request
pub fn request_signature_help(lsp_client: &LspClient, uri: &Url, position: Position) {
    if lsp_client.capabilities.supports_signature_help() {
        lsp_client.send(LspMessage::SignatureHelp {
            uri: uri.clone(),
            position,
        });
    }
}

/// Helper to send code action request
pub fn request_code_actions(
    lsp_client: &LspClient,
    uri: &Url,
    range: Range,
    diagnostics: Vec<Diagnostic>,
) {
    if lsp_client.capabilities.supports_code_actions() {
        lsp_client.send(LspMessage::CodeAction {
            uri: uri.clone(),
            range,
            diagnostics,
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
                eprintln!("[LSP] Code action has workspace edit: {:?}", edit);
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
    lsp_client: Res<LspClient>,
    editor_state: Res<CodeEditorState>,
    lsp_sync: Res<LspSyncState>,
    mut highlight_state: ResMut<DocumentHighlightState>,
) {
    if !lsp_client.is_ready() || !lsp_client.capabilities.supports_document_highlight() {
        return;
    }

    let Some(uri) = &lsp_sync.document_uri else {
        return;
    };

    // Check if cursor moved
    if editor_state.cursor_pos == highlight_state.cursor_position && highlight_state.visible {
        return;
    }

    // Debounce: wait 150ms after cursor stops moving
    if let Some(ref mut timer) = highlight_state.debounce_timer {
        timer.tick(time.delta());
        if !timer.is_finished() {
            return;
        }
    } else {
        // Start debounce timer
        highlight_state.debounce_timer = Some(Timer::from_seconds(0.15, TimerMode::Once));
        highlight_state.cursor_position = editor_state.cursor_pos;
        return;
    }

    // Clear timer and send request
    highlight_state.debounce_timer = None;
    highlight_state.cursor_position = editor_state.cursor_pos;

    // Convert cursor position to LSP position
    let cursor_pos = editor_state.cursor_pos.min(editor_state.rope.len_chars());
    let line = editor_state.rope.char_to_line(cursor_pos);
    let line_start = editor_state.rope.line_to_char(line);
    let character = cursor_pos - line_start;

    lsp_client.send(LspMessage::DocumentHighlight {
        uri: uri.clone(),
        position: Position {
            line: line as u32,
            character: character as u32,
        },
    });
}

/// Helper to request prepare rename
pub fn request_prepare_rename(lsp_client: &LspClient, uri: &Url, position: Position) {
    eprintln!("[LSP] request_prepare_rename called, supports_prepare: {}, supports_rename: {}",
        lsp_client.capabilities.supports_prepare_rename(),
        lsp_client.capabilities.supports_rename());

    if lsp_client.capabilities.supports_prepare_rename() {
        eprintln!("[LSP] Sending PrepareRename request");
        lsp_client.send(LspMessage::PrepareRename {
            uri: uri.clone(),
            position,
        });
    } else if lsp_client.capabilities.supports_rename() {
        // Server doesn't support prepare, go straight to rename dialog
        eprintln!("[LSP] Server doesn't support prepare rename, but supports rename");
        // The caller should handle this case
    } else {
        eprintln!("[LSP] Server doesn't support rename at all");
    }
}

/// Helper to execute rename
pub fn execute_rename(lsp_client: &LspClient, uri: &Url, position: Position, new_name: String) {
    if lsp_client.capabilities.supports_rename() {
        lsp_client.send(LspMessage::Rename {
            uri: uri.clone(),
            position,
            new_name,
        });
    }
}
