use crate::types::*;
use crate::settings::IndentationSettings;
#[cfg(feature = "lsp")]
use crate::settings::LspSettings;
use super::keybindings::EditorAction;
use super::cursor::*;
use arboard::Clipboard;

#[cfg(feature = "lsp")]
use crate::lsp;

/// Result of executing an action
pub struct ActionResult {
    /// Whether text content was modified
    pub text_changed: bool,
    /// Whether cursor moved horizontally (for LSP completion dismissal)
    pub horizontal_move: bool,
}

/// Insert a character at cursor position
pub fn insert_char(state: &mut CodeEditorState, c: char) {
    // Delete selection if exists
    if state.selection_start.is_some() && state.selection_end.is_some() {
        delete_selection(state);
    }

    state.insert_char(c);
}

/// Insert a closing character at cursor position without moving the cursor
/// Used for bracket/quote auto-close
pub fn insert_closing_char(state: &mut CodeEditorState, c: char) {
    let cursor_pos = state.cursor_pos.min(state.rope.len_chars());

    // Record for incremental parsing
    #[cfg(feature = "tree-sitter")]
    {
        let start_byte = state.rope.char_to_byte(cursor_pos);
        let char_len = c.len_utf8();
        state.record_edit(start_byte, start_byte, start_byte + char_len);
    }

    // Insert at cursor position
    state.rope.insert_char(cursor_pos, c);

    // Don't move cursor - it stays between the brackets
    // OPTIMIZATION: Use debounce instead of immediate update
    state.pending_update = true;
    state.content_version += 1;

    // Mark only current line as dirty (not entire rest of file!)
    let line_idx = state.rope.char_to_line(cursor_pos);
    let new_line_count = state.rope.len_lines();
    state.dirty_lines = Some(line_idx..(line_idx + 1).min(new_line_count));
    state.previous_line_count = new_line_count;
}

/// Get the closing bracket for an opening bracket
pub fn get_closing_bracket(open: char, pairs: &[(char, char)]) -> Option<char> {
    pairs.iter().find(|(o, _)| *o == open).map(|(_, c)| *c)
}

/// Get the matching quote character (quotes are self-closing)
pub fn get_closing_quote(c: char) -> Option<char> {
    match c {
        '"' | '\'' | '`' => Some(c),
        _ => None,
    }
}

/// Check if we should skip inserting a closing character
/// (e.g., when cursor is already followed by the same character)
pub fn should_skip_auto_close(state: &CodeEditorState, closing: char) -> bool {
    let cursor_pos = state.cursor_pos;
    if cursor_pos >= state.rope.len_chars() {
        return false;
    }
    // If the next character is the same as what we'd insert, skip
    state.rope.char(cursor_pos) == closing
}

/// Delete selected text (with undo recording)
pub fn delete_selection(state: &mut CodeEditorState) {
    delete_selection_with_history(state, true);
}

/// Delete selected text with optional history recording
fn delete_selection_with_history(state: &mut CodeEditorState, record_history: bool) {
    if let (Some(start), Some(end)) = (state.selection_start, state.selection_end) {
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };

        let cursor_before = state.cursor_pos;

        // Get the text being deleted for undo
        let deleted_text: String = state.rope.slice(start..end).chars().collect();

        // Remove selected text
        let start_byte = state.rope.char_to_byte(start);
        let end_byte = state.rope.char_to_byte(end);

        // Record edit for incremental parsing
        #[cfg(feature = "tree-sitter")]
        state.record_edit(start_byte, end_byte, start_byte);

        state.rope.remove(start_byte..end_byte);

        // Move cursor to start of selection
        state.cursor_pos = start;

        // Record for undo
        if record_history && !deleted_text.is_empty() {
            state.history.record(EditOperation {
                removed_text: deleted_text,
                inserted_text: String::new(),
                position: start,
                cursor_before,
                cursor_after: start,
                kind: EditKind::Other, // Selection deletion is its own transaction
            });
        }

        // Clear selection
        state.selection_start = None;
        state.selection_end = None;

        state.needs_update = true;
        state.pending_update = false;
        state.content_version += 1;
        state.dirty_lines = None;
        state.previous_line_count = state.rope.len_lines();
    }
}

/// Apply selected completion item
#[cfg(feature = "lsp")]
pub fn apply_completion(
    state: &mut CodeEditorState,
    completion_state: &mut lsp::CompletionState,
) {
    // Get filtered items and select from that list
    let filtered = completion_state.filtered_items();
    if let Some(item) = filtered.get(completion_state.selected_index) {
        let start = completion_state.start_char_index;
        let end = state.cursor_pos;
        let insert_text = item.insert_text().to_string();

        // Ensure valid range
        if start <= end && end <= state.rope.len_chars() {
            let start_byte = state.rope.char_to_byte(start);
            let end_byte = state.rope.char_to_byte(end);
            let new_end_byte = start_byte + insert_text.len();

            // Record edit for incremental parsing (remove + insert = replace)
            state.record_edit(start_byte, end_byte, new_end_byte);

            state.rope.remove(start_byte..end_byte);
            state.rope.insert(start, &insert_text);

            state.cursor_pos = start + insert_text.chars().count();
            state.needs_update = true;
            state.pending_update = false;
            state.content_version += 1;

            // Mark lines as dirty for highlighting update
            let line_idx = state.rope.char_to_line(start);
            let new_line_count = state.rope.len_lines();
            state.dirty_lines = Some(line_idx..(line_idx + 1).min(new_line_count));
            state.previous_line_count = new_line_count;
        }
    }
    completion_state.visible = false;
    completion_state.filter.clear();
    completion_state.scroll_offset = 0;
}

/// Find the start of the current word (for auto-triggering completion)
#[cfg(feature = "lsp")]
pub fn find_word_start(rope: &ropey::Rope, cursor_pos: usize) -> usize {
    if cursor_pos == 0 {
        return 0;
    }

    let mut pos = cursor_pos;
    while pos > 0 {
        let prev_char = rope.char(pos - 1);
        if prev_char.is_alphanumeric() || prev_char == '_' {
            pos -= 1;
        } else {
            break;
        }
    }
    pos
}

/// Update the completion filter based on text typed since start_char_index
#[cfg(feature = "lsp")]
pub fn update_completion_filter(
    state: &CodeEditorState,
    completion_state: &mut lsp::CompletionState,
) {
    let cursor_pos = state.cursor_pos.min(state.rope.len_chars());
    let start = completion_state.start_char_index;

    if cursor_pos > start && start <= state.rope.len_chars() {
        // Extract the filter text from start_char_index to cursor
        let filter_text: String = state.rope.slice(start..cursor_pos).chars().collect();
        completion_state.filter = filter_text;
        // Reset selection and scroll when filter changes
        completion_state.selected_index = 0;
        completion_state.scroll_offset = 0;

        #[cfg(debug_assertions)]
        eprintln!("[LSP] Filter updated: '{}'", completion_state.filter);
    } else {
        completion_state.filter.clear();
        completion_state.scroll_offset = 0;
    }
}

/// Request completion from LSP
#[cfg(feature = "lsp")]
pub fn request_completion(
    state: &CodeEditorState,
    lsp_client: &lsp::LspClient,
    completion_state: &mut lsp::CompletionState,
    lsp_sync: &lsp::LspSyncState,
) {
    use lsp_types::Position;
    use crate::lsp::LspMessage;

    let cursor_pos = state.cursor_pos.min(state.rope.len_chars());
    let line_index = state.rope.char_to_line(cursor_pos);
    let char_in_line_index = cursor_pos - state.rope.line_to_char(line_index);

    let lsp_position = Position {
        line: line_index as u32,
        character: char_in_line_index as u32,
    };

    if let Some(uri) = &lsp_sync.document_uri {
        #[cfg(debug_assertions)]
        eprintln!("[LSP] Requesting completion at line={}, char={}, visible={}, start_idx={}",
            lsp_position.line, lsp_position.character, completion_state.visible, completion_state.start_char_index);

        lsp_client.send(LspMessage::Completion {
            uri: uri.clone(),
            position: lsp_position,
        });

        // Only set start_char_index when first opening completion
        if !completion_state.visible {
            completion_state.start_char_index = cursor_pos;
            completion_state.items.clear();
            completion_state.selected_index = 0;
            completion_state.filter.clear();
        }

        // Always update word completions from the document
        completion_state.update_word_completions(&state.rope, cursor_pos);

        completion_state.visible = true;
    } else {
        // No LSP document URI - still provide word completions
        if !completion_state.visible {
            completion_state.start_char_index = cursor_pos;
            completion_state.items.clear();
            completion_state.selected_index = 0;
            completion_state.filter.clear();
        }

        // Populate word completions even without LSP
        completion_state.update_word_completions(&state.rope, cursor_pos);
        completion_state.visible = true;

        #[cfg(debug_assertions)]
        eprintln!("[bevy_code_editor] No LSP document URI - using word completions only ({} words)",
            completion_state.word_items.len());
    }
}

/// Send textDocument/didChange notification to LSP
#[cfg(feature = "lsp")]
pub fn send_did_change(
    state: &CodeEditorState,
    lsp_client: &lsp::LspClient,
    lsp_sync: &mut lsp::LspSyncState,
) {
    use crate::lsp::LspMessage;

    if let Some(uri) = &lsp_sync.document_uri {
        // Increment version for each change
        lsp_sync.document_version += 1;
        let version = lsp_sync.document_version;

        // Full text sync for simplicity
        // OPTIMIZATION: Use rope chunks instead of full to_string() conversion
        let change = lsp_types::TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: state.rope.chunks().collect(),
        };

        lsp_client.send(LspMessage::DidChange {
            uri: uri.clone(),
            version,
            changes: vec![change],
        });

        #[cfg(debug_assertions)]
        eprintln!("[LSP] DidChange sent, version={}", version);
    }
}

/// Core action execution - shared between LSP and non-LSP builds
fn execute_action_core(
    state: &mut CodeEditorState,
    action: EditorAction,
    indentation: &IndentationSettings,
    find_state: &mut FindState,
    goto_line_state: &mut GotoLineState,
    fold_state: &mut FoldState,
) -> ActionResult {
    let mut result = ActionResult {
        text_changed: false,
        horizontal_move: false,
    };

    match action {
        EditorAction::InsertNewline => {
            insert_char(state, '\n');
            result.text_changed = true;
        }
        EditorAction::InsertTab => {
            for _ in 0..indentation.tab_width {
                insert_char(state, ' ');
            }
            result.text_changed = true;
        }

        EditorAction::DeleteBackward => {
            if state.selection_start.is_some() {
                delete_selection(state);
            } else {
                state.delete_backward();
            }
            result.text_changed = true;
        }
        EditorAction::DeleteForward => {
            if state.selection_start.is_some() {
                delete_selection(state);
            } else {
                state.delete_forward();
            }
            result.text_changed = true;
        }
        EditorAction::DeleteWordBackward => {
            if state.selection_start.is_some() {
                delete_selection(state);
            } else {
                delete_word_backward(state);
            }
            result.text_changed = true;
        }
        EditorAction::DeleteWordForward => {
            if state.selection_start.is_some() {
                delete_selection(state);
            } else {
                delete_word_forward(state);
            }
            result.text_changed = true;
        }
        EditorAction::DeleteLine => {
            // TODO: Implement line deletion
        }

        EditorAction::MoveCursorLeft => {
            state.selection_start = None;
            state.selection_end = None;
            state.move_cursor(-1);
            result.horizontal_move = true;
        }
        EditorAction::MoveCursorRight => {
            state.selection_start = None;
            state.selection_end = None;
            state.move_cursor(1);
            result.horizontal_move = true;
        }
        EditorAction::MoveCursorUp => {
            state.selection_start = None;
            state.selection_end = None;
            move_cursor_up(state);
        }
        EditorAction::MoveCursorDown => {
            state.selection_start = None;
            state.selection_end = None;
            move_cursor_down(state);
        }
        EditorAction::MoveCursorWordLeft => {
            state.selection_start = None;
            state.selection_end = None;
            move_cursor_word_left(state);
            result.horizontal_move = true;
        }
        EditorAction::MoveCursorWordRight => {
            state.selection_start = None;
            state.selection_end = None;
            move_cursor_word_right(state);
            result.horizontal_move = true;
        }
        EditorAction::MoveCursorLineStart => {
            state.selection_start = None;
            state.selection_end = None;
            move_cursor_line_start(state);
        }
        EditorAction::MoveCursorLineEnd => {
            state.selection_start = None;
            state.selection_end = None;
            move_cursor_line_end(state);
        }
        EditorAction::MoveCursorDocumentStart => {
            state.selection_start = None;
            state.selection_end = None;
            state.cursor_pos = 0;
        }
        EditorAction::MoveCursorDocumentEnd => {
            state.selection_start = None;
            state.selection_end = None;
            state.cursor_pos = state.rope.len_chars();
        }
        EditorAction::MoveCursorPageUp => {
            state.selection_start = None;
            state.selection_end = None;
            // TODO: Implement page up
        }
        EditorAction::MoveCursorPageDown => {
            state.selection_start = None;
            state.selection_end = None;
            // TODO: Implement page down
        }

        EditorAction::SelectLeft => {
            init_selection(state);
            state.move_cursor(-1);
            state.selection_end = Some(state.cursor_pos);
        }
        EditorAction::SelectRight => {
            init_selection(state);
            state.move_cursor(1);
            state.selection_end = Some(state.cursor_pos);
        }
        EditorAction::SelectUp => {
            init_selection(state);
            move_cursor_up(state);
            state.selection_end = Some(state.cursor_pos);
        }
        EditorAction::SelectDown => {
            init_selection(state);
            move_cursor_down(state);
            state.selection_end = Some(state.cursor_pos);
        }
        EditorAction::SelectWordLeft => {
            init_selection(state);
            move_cursor_word_left(state);
            state.selection_end = Some(state.cursor_pos);
        }
        EditorAction::SelectWordRight => {
            init_selection(state);
            move_cursor_word_right(state);
            state.selection_end = Some(state.cursor_pos);
        }
        EditorAction::SelectLineStart => {
            init_selection(state);
            move_cursor_line_start(state);
            state.selection_end = Some(state.cursor_pos);
        }
        EditorAction::SelectLineEnd => {
            init_selection(state);
            move_cursor_line_end(state);
            state.selection_end = Some(state.cursor_pos);
        }
        EditorAction::SelectAll => {
            state.selection_start = Some(0);
            state.selection_end = Some(state.rope.len_chars());
            state.cursor_pos = state.rope.len_chars();
        }
        EditorAction::ClearSelection => {
            state.selection_start = None;
            state.selection_end = None;
        }

        EditorAction::Copy => {
            if let (Some(s), Some(e)) = (state.selection_start, state.selection_end) {
                let (start, end) = if s < e { (s, e) } else { (e, s) };
                let start = start.min(state.rope.len_chars());
                let end = end.min(state.rope.len_chars());
                let text = state.rope.slice(start..end).to_string();
                if let Ok(mut clipboard) = Clipboard::new() {
                    let _ = clipboard.set_text(text);
                }
            }
        }
        EditorAction::Cut => {
            if let (Some(s), Some(e)) = (state.selection_start, state.selection_end) {
                let (start, end) = if s < e { (s, e) } else { (e, s) };
                let start = start.min(state.rope.len_chars());
                let end = end.min(state.rope.len_chars());
                let selected_text = state.rope.slice(start..end).to_string();
                let cursor_before = state.cursor_pos;

                // Copy to clipboard
                if let Ok(mut clipboard) = Clipboard::new() {
                    let _ = clipboard.set_text(selected_text.clone());
                }

                // Delete the selection
                let start_byte = state.rope.char_to_byte(start);
                let end_byte = state.rope.char_to_byte(end);

                // Record edit for incremental parsing
                #[cfg(feature = "tree-sitter")]
                state.record_edit(start_byte, end_byte, start_byte);

                state.rope.remove(start_byte..end_byte);
                state.cursor_pos = start;

                // Record for undo
                state.history.record(EditOperation {
                    removed_text: selected_text,
                    inserted_text: String::new(),
                    position: start,
                    cursor_before,
                    cursor_after: start,
                    kind: EditKind::Other, // Cut is its own transaction
                });

                state.selection_start = None;
                state.selection_end = None;
                state.needs_update = true;
                state.pending_update = false;
                state.content_version += 1;

                let new_line_count = state.rope.len_lines();
                let line_idx = state.rope.char_to_line(start);
                state.dirty_lines = Some(line_idx..(line_idx + 1).min(new_line_count));
                state.previous_line_count = new_line_count;

                result.text_changed = true;
            }
        }
        EditorAction::Paste => {
            {
                if let Ok(mut clipboard) = Clipboard::new() {
                    if let Ok(text) = clipboard.get_text() {
                        let cursor_before = state.cursor_pos;
                        let mut deleted_text = String::new();
                        let paste_position;

                        // Delete selection if any
                        if let (Some(start), Some(end)) = (state.selection_start, state.selection_end) {
                            let (start, end) = if start < end { (start, end) } else { (end, start) };
                            let start = start.min(state.rope.len_chars());
                            let end = end.min(state.rope.len_chars());

                            deleted_text = state.rope.slice(start..end).to_string();

                            let start_byte = state.rope.char_to_byte(start);
                            let end_byte = state.rope.char_to_byte(end);
                            let new_end_byte = start_byte + text.len();

                            // Record combined edit for incremental parsing (delete + insert)
                            #[cfg(feature = "tree-sitter")]
                            state.record_edit(start_byte, end_byte, new_end_byte);

                            state.rope.remove(start_byte..end_byte);
                            state.cursor_pos = start;
                            state.selection_start = None;
                            state.selection_end = None;
                            paste_position = start;
                        } else {
                            paste_position = state.cursor_pos.min(state.rope.len_chars());

                            // Record insert-only edit for incremental parsing
                            #[cfg(feature = "tree-sitter")]
                            {
                                let start_byte = state.rope.char_to_byte(paste_position);
                                state.record_edit(start_byte, start_byte, start_byte + text.len());
                            }
                        }

                        // Insert pasted text
                        let line_idx = state.rope.char_to_line(paste_position);

                        state.rope.insert(paste_position, &text);
                        state.cursor_pos = paste_position + text.chars().count();
                        state.needs_update = true;
                        state.pending_update = false;
                        state.content_version += 1;

                        // Record for undo (combined delete selection + insert paste)
                        state.history.record(EditOperation {
                            removed_text: deleted_text,
                            inserted_text: text.clone(),
                            position: paste_position,
                            cursor_before,
                            cursor_after: state.cursor_pos,
                            kind: EditKind::Paste, // Paste is always its own transaction
                        });

                        let new_line_count = state.rope.len_lines();
                        state.dirty_lines = Some(line_idx..(line_idx + 1).min(new_line_count));
                        state.previous_line_count = new_line_count;

                        result.text_changed = true;
                    }
                }
            }
        }

        EditorAction::Undo => {
            if state.undo() {
                result.text_changed = true;
            }
        }
        EditorAction::Redo => {
            if state.redo() {
                result.text_changed = true;
            }
        }

        EditorAction::Find => {
            // Search for selected text or word at cursor
            if let (Some(start), Some(end)) = (state.selection_start, state.selection_end) {
                let (start, end) = if start < end { (start, end) } else { (end, start) };
                let query: String = state.rope.slice(start..end).chars().collect();
                if !query.is_empty() {
                    find_state.query = query;
                    find_state.active = true;
                    find_state.search(&state.rope);
                }
            } else {
                // Find word at cursor
                let cursor = state.cursor_pos.min(state.rope.len_chars());
                if cursor < state.rope.len_chars() {
                    let c = state.rope.char(cursor);
                    if c.is_alphanumeric() || c == '_' {
                        // Find word boundaries
                        let mut start = cursor;
                        while start > 0 {
                            let prev = state.rope.char(start - 1);
                            if prev.is_alphanumeric() || prev == '_' {
                                start -= 1;
                            } else {
                                break;
                            }
                        }
                        let mut end = cursor;
                        while end < state.rope.len_chars() {
                            let ch = state.rope.char(end);
                            if ch.is_alphanumeric() || ch == '_' {
                                end += 1;
                            } else {
                                break;
                            }
                        }
                        let query: String = state.rope.slice(start..end).chars().collect();
                        if !query.is_empty() {
                            find_state.query = query;
                            find_state.active = true;
                            find_state.search(&state.rope);
                        }
                    }
                }
            }
        }
        EditorAction::FindNext => {
            if find_state.active && !find_state.matches.is_empty() {
                find_state.find_next(state.cursor_pos);
                // Move cursor to the match
                if let Some(m) = find_state.current_match() {
                    state.cursor_pos = m.start;
                    state.selection_start = Some(m.start);
                    state.selection_end = Some(m.end);
                    state.pending_update = true;
                }
            }
        }
        EditorAction::FindPrevious => {
            if find_state.active && !find_state.matches.is_empty() {
                find_state.find_previous(state.cursor_pos);
                // Move cursor to the match
                if let Some(m) = find_state.current_match() {
                    state.cursor_pos = m.start;
                    state.selection_start = Some(m.start);
                    state.selection_end = Some(m.end);
                    state.pending_update = true;
                }
            }
        }
        EditorAction::Replace => {
            // TODO: Implement replace
        }
        EditorAction::RequestCompletion => {
            // Handled by LSP wrapper
        }
        EditorAction::GotoDefinition => {
            // Handled by mouse input
        }
        EditorAction::RenameSymbol => {
            // Handled by LSP wrapper - triggers prepare rename
        }
        EditorAction::GotoLine => {
            // Toggle goto line dialog
            goto_line_state.active = !goto_line_state.active;
            if goto_line_state.active {
                goto_line_state.input.clear();
            }
        }

        // Multi-cursor actions
        EditorAction::AddCursorAtNextOccurrence => {
            // Sync the cursors from primary first
            state.sync_cursors_from_primary();
            state.add_cursor_at_next_occurrence();
        }
        EditorAction::AddCursorAbove => {
            // Add cursor on the line above
            state.sync_cursors_from_primary();
            add_cursor_above(state);
        }
        EditorAction::AddCursorBelow => {
            // Add cursor on the line below
            state.sync_cursors_from_primary();
            add_cursor_below(state);
        }
        EditorAction::ClearSecondaryCursors => {
            // Clear all but primary cursor
            if state.has_multiple_cursors() {
                state.clear_secondary_cursors();
            }
        }

        // Code folding actions
        EditorAction::ToggleFold => {
            let line = state.rope.char_to_line(state.cursor_pos);
            fold_state.toggle_fold_at_line(line);
            state.pending_update = true;
        }
        EditorAction::Fold => {
            let line = state.rope.char_to_line(state.cursor_pos);
            fold_state.fold_at_line(line);
            state.pending_update = true;
        }
        EditorAction::Unfold => {
            let line = state.rope.char_to_line(state.cursor_pos);
            fold_state.unfold_at_line(line);
            state.pending_update = true;
        }
        EditorAction::FoldAll => {
            fold_state.fold_all();
            state.pending_update = true;
        }
        EditorAction::UnfoldAll => {
            fold_state.unfold_all();
            state.pending_update = true;
        }

        // File operations are handled in keyboard.rs before execute_action is called
        // These emit events for the host app to handle
        EditorAction::Save | EditorAction::Open => {
            // No-op here - handled via events in keyboard input system
        }
    }

    result
}

/// Add a cursor on the line above the primary cursor
fn add_cursor_above(state: &mut CodeEditorState) {
    if state.cursors.is_empty() {
        return;
    }

    // Get the primary cursor's line and column
    let primary_pos = state.cursors[0].position;
    let line_idx = state.rope.char_to_line(primary_pos);

    if line_idx == 0 {
        // Already at top, can't go up
        return;
    }

    let line_start = state.rope.line_to_char(line_idx);
    let col_offset = primary_pos - line_start;

    // Find position on the line above
    let prev_line_start = state.rope.line_to_char(line_idx - 1);
    let prev_line_len = state.rope.line(line_idx - 1).len_chars().saturating_sub(1); // Exclude newline
    let new_pos = prev_line_start + col_offset.min(prev_line_len);

    state.add_cursor(new_pos);
}

/// Add a cursor on the line below the primary cursor
fn add_cursor_below(state: &mut CodeEditorState) {
    if state.cursors.is_empty() {
        return;
    }

    // Get the primary cursor's line and column
    let primary_pos = state.cursors[0].position;
    let line_idx = state.rope.char_to_line(primary_pos);

    if line_idx + 1 >= state.rope.len_lines() {
        // Already at bottom, can't go down
        return;
    }

    let line_start = state.rope.line_to_char(line_idx);
    let col_offset = primary_pos - line_start;

    // Find position on the line below
    let next_line_start = state.rope.line_to_char(line_idx + 1);
    let next_line_len = state.rope.line(line_idx + 1).len_chars().saturating_sub(1); // Exclude newline
    let new_pos = next_line_start + col_offset.min(next_line_len);

    state.add_cursor(new_pos);
}

/// Execute an editor action (Non-LSP version)
#[cfg(not(feature = "lsp"))]
pub fn execute_action(
    state: &mut CodeEditorState,
    action: EditorAction,
    indentation: &IndentationSettings,
    find_state: &mut FindState,
    goto_line_state: &mut GotoLineState,
    fold_state: &mut FoldState,
) {
    // Handle Escape to clear multi-cursors, find mode, or goto line mode
    if action == EditorAction::ClearSelection {
        // First priority: clear secondary cursors if we have multiple
        if state.has_multiple_cursors() {
            state.clear_secondary_cursors();
            return;
        }
        if goto_line_state.active {
            goto_line_state.clear();
            return;
        }
        if find_state.active {
            find_state.clear();
            state.selection_start = None;
            state.selection_end = None;
            state.pending_update = true;
            return;
        }
    }

    let _ = execute_action_core(state, action, indentation, find_state, goto_line_state, fold_state);
}

/// Execute an editor action (LSP version)
#[cfg(feature = "lsp")]
pub fn execute_action(
    state: &mut CodeEditorState,
    action: EditorAction,
    indentation: &IndentationSettings,
    lsp: &LspSettings,
    find_state: &mut FindState,
    goto_line_state: &mut GotoLineState,
    fold_state: &mut FoldState,
    lsp_client: &lsp::LspClient,
    completion_state: &mut lsp::CompletionState,
    lsp_sync: &mut lsp::LspSyncState,
) {
    // Handle Escape to clear multi-cursors, goto line mode, find mode, or completion
    if action == EditorAction::ClearSelection {
        // First priority: clear secondary cursors if we have multiple
        if state.has_multiple_cursors() {
            state.clear_secondary_cursors();
            return;
        }
        if goto_line_state.active {
            goto_line_state.clear();
            return;
        }
        if find_state.active {
            find_state.clear();
            state.selection_start = None;
            state.selection_end = None;
            state.pending_update = true;
            return;
        }
    }

    // Handle Completion UI Navigation first
    let filtered_count = completion_state.filtered_items().len();
    let max_visible = lsp.completion.max_visible_items;

    if completion_state.visible && filtered_count > 0 {
        match action {
            EditorAction::MoveCursorUp => {
                if completion_state.selected_index > 0 {
                    completion_state.selected_index -= 1;
                } else {
                    completion_state.selected_index = filtered_count.saturating_sub(1);
                }
                completion_state.ensure_selected_visible_with_max(max_visible);
                return;
            }
            EditorAction::MoveCursorDown => {
                if completion_state.selected_index + 1 < filtered_count {
                    completion_state.selected_index += 1;
                } else {
                    completion_state.selected_index = 0;
                }
                completion_state.ensure_selected_visible_with_max(max_visible);
                return;
            }
            EditorAction::InsertNewline | EditorAction::InsertTab => {
                apply_completion(state, completion_state);
                send_did_change(state, lsp_client, lsp_sync);
                return;
            }
            EditorAction::ClearSelection => {
                completion_state.visible = false;
                completion_state.filter.clear();
                completion_state.scroll_offset = 0;
                return;
            }
            _ => {}
        }
    }

    // Handle LSP-specific actions
    if action == EditorAction::RequestCompletion {
        request_completion(state, lsp_client, completion_state, lsp_sync);
        return;
    }

    // Execute the core action
    let result = execute_action_core(state, action, indentation, find_state, goto_line_state, fold_state);

    // LSP-specific post-processing: dismiss completion on horizontal move
    if result.horizontal_move {
        completion_state.visible = false;
    }

    // Update completion filter on delete backward
    if action == EditorAction::DeleteBackward && completion_state.visible {
        if state.cursor_pos > completion_state.start_char_index {
            update_completion_filter(state, completion_state);
        } else if state.cursor_pos == completion_state.start_char_index {
            completion_state.filter.clear();
            completion_state.selected_index = 0;
        } else {
            completion_state.visible = false;
            completion_state.filter.clear();
        }
    }

    // Notify LSP of text changes
    if result.text_changed {
        send_did_change(state, lsp_client, lsp_sync);
    }
}