use super::cursor::*;
use super::editor_ops::{add_cursor_at_next_occurrence, move_cursor};
use super::keybindings::EditorAction;
use crate::settings::IndentationSettings;
#[cfg(feature = "lsp")]
use crate::settings::LspSettings;
use crate::text_view::TextViewState;
use crate::types::*;
use arboard::Clipboard;
use ropey::Rope;

#[cfg(feature = "lsp")]
use crate::lsp_ui::state::LspCompletionPopup;
#[cfg(feature = "lsp")]
use bevy::log::trace;
#[cfg(feature = "lsp")]
use bevy_lsp::{LspClient, LspDocument, LspMessage};

/// Result of executing an action
pub struct ActionResult {
    /// Whether text content was modified
    pub text_changed: bool,
    /// Whether cursor moved horizontally (for LSP completion dismissal)
    pub horizontal_move: bool,
}

/// Bundled refs to the four LSP pieces that co-travel through `execute_action`:
/// settings, transport client, completion popup state, and the per-editor
/// LspDocument (URI / version). Mirrors `EditorBuf` but for LSP-specific
/// borrows; only constructed when the `lsp` feature is enabled. `document` is
/// `Option` because a freshly spawned editor may not have an `LspDocument`
/// inserted yet.
#[cfg(feature = "lsp")]
pub struct LspBuf<'a> {
    pub settings: &'a LspSettings,
    pub client: &'a LspClient,
    pub completion: &'a mut LspCompletionPopup,
    pub document: Option<&'a mut LspDocument>,
}

/// Insert a character at cursor position
pub fn insert_char(
    sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    syntax: &mut SyntaxCacheState,
    display: &mut EditorDisplayState,
    cursor: &mut CursorState,
    tv: &mut TextViewState,
    c: char,
) {
    // Delete selection if exists
    if sel.selection_start.is_some() && sel.selection_end.is_some() {
        delete_selection(sel, hist, syntax, display, cursor, tv);
    }

    hist.insert_char(sel, syntax, display, cursor, tv, c);
}

/// Insert a closing character at cursor position without moving the cursor
/// Used for bracket/quote auto-close
pub fn insert_closing_char(
    cursor: &CursorState,
    tv: &mut TextViewState,
    c: char,
) {
    let cursor_pos = cursor.cursor_pos.min(tv.rope.len_chars());

    // Insert at cursor position
    tv.rope.insert_char(cursor_pos, c);

    // Don't move cursor - it stays between the brackets
    tv.content_version += 1;
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
pub fn should_skip_auto_close(cursor: &CursorState, rope: &Rope, closing: char) -> bool {
    let cursor_pos = cursor.cursor_pos;
    if cursor_pos >= rope.len_chars() {
        return false;
    }
    // If the next character is the same as what we'd insert, skip
    rope.char(cursor_pos) == closing
}

/// Delete selected text (with undo recording)
pub fn delete_selection(
    sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    syntax: &mut SyntaxCacheState,
    display: &mut EditorDisplayState,
    cursor: &mut CursorState,
    tv: &mut TextViewState,
) {
    delete_selection_with_history(sel, hist, syntax, display, cursor, tv, true);
}

/// Delete selected text with optional history recording
fn delete_selection_with_history(
    sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    _syntax: &mut SyntaxCacheState,
    _display: &mut EditorDisplayState,
    cursor: &mut CursorState,
    tv: &mut TextViewState,
    record_history: bool,
) {
    if let (Some(start), Some(end)) = (sel.selection_start, sel.selection_end) {
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };

        let cursor_before = cursor.cursor_pos;

        // Get the text being deleted for undo
        let deleted_text: String = tv.rope.slice(start..end).chars().collect();

        // Remove selected text
        let start_byte = tv.rope.char_to_byte(start);
        let end_byte = tv.rope.char_to_byte(end);

        tv.rope.remove(start_byte..end_byte);

        // Move cursor to start of selection
        cursor.cursor_pos = start;

        // Record for undo
        if record_history && !deleted_text.is_empty() {
            hist.history.record(EditOperation {
                removed_text: deleted_text,
                inserted_text: String::new(),
                position: start,
                cursor_before,
                cursor_after: start,
                kind: EditKind::Other, // Selection deletion is its own transaction
            });
        }

        // Clear selection
        sel.selection_start = None;
        sel.selection_end = None;

        tv.content_version += 1;
    }
}

/// Apply selected completion item
#[cfg(feature = "lsp")]
pub fn apply_completion(
    cursor: &mut CursorState,
    tv: &mut TextViewState,
    completion_state: &mut LspCompletionPopup,
) {
    // Get filtered items and select from that list
    let filtered = completion_state.filtered_items();
    if let Some(item) = filtered.get(completion_state.selected_index) {
        let start = completion_state.start_char_index;
        let end = cursor.cursor_pos;
        let insert_text = item.insert_text().to_string();

        // Ensure valid range
        if start <= end && end <= tv.rope.len_chars() {
            let start_byte = tv.rope.char_to_byte(start);
            let end_byte = tv.rope.char_to_byte(end);

            tv.rope.remove(start_byte..end_byte);
            tv.rope.insert(start, &insert_text);

            cursor.cursor_pos = start + insert_text.chars().count();
            tv.content_version += 1;

            // Mark lines as dirty for highlighting update
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
    cursor: &CursorState,
    rope: &Rope,
    completion_state: &mut LspCompletionPopup,
) {
    let cursor_pos = cursor.cursor_pos.min(rope.len_chars());
    let start = completion_state.start_char_index;

    if cursor_pos > start && start <= rope.len_chars() {
        // Extract the filter text from start_char_index to cursor
        let filter_text: String = rope.slice(start..cursor_pos).chars().collect();
        completion_state.filter = filter_text;
        // Reset selection and scroll when filter changes
        completion_state.selected_index = 0;
        completion_state.scroll_offset = 0;

        trace!("[LSP] Filter updated: '{}'", completion_state.filter);
    } else {
        completion_state.filter.clear();
        completion_state.scroll_offset = 0;
    }
}

/// Request completion from LSP
#[cfg(feature = "lsp")]
pub fn request_completion(
    cursor: &CursorState,
    rope: &Rope,
    lsp_client: &LspClient,
    completion_state: &mut LspCompletionPopup,
    lsp_document: Option<&LspDocument>,
) {
    let cursor_pos = cursor.cursor_pos.min(rope.len_chars());
    let lsp_position = bevy_lsp::rope_char_to_lsp_position(
        rope,
        cursor_pos,
        bevy_lsp::PositionEncoding::Utf16,
    );

    if let Some(doc) = lsp_document {
        trace!(
            "[LSP] Requesting completion at line={}, char={}, visible={}, start_idx={}",
            lsp_position.line,
            lsp_position.character,
            completion_state.visible,
            completion_state.start_char_index
        );

        lsp_client.send(LspMessage::Completion {
            uri: doc.uri.clone(),
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
        completion_state.update_word_completions(rope, cursor_pos);

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
        completion_state.update_word_completions(rope, cursor_pos);
        completion_state.visible = true;

        trace!(
            "[bevy_code_editor] No LSP document URI - using word completions only ({} words)",
            completion_state.word_items.len()
        );
    }
}

/// Send textDocument/didChange notification to LSP
#[cfg(feature = "lsp")]
pub fn send_did_change(rope: &Rope, lsp_client: &LspClient, lsp_document: Option<&mut LspDocument>) {
    let Some(doc) = lsp_document else {
        return;
    };
    // Increment version for each change
    let version = doc.bump_version();

    // Full text sync for simplicity
    // OPTIMIZATION: Use rope chunks instead of full to_string() conversion
    let change = lsp_types::TextDocumentContentChangeEvent {
        range: None,
        range_length: None,
        text: rope.chunks().collect(),
    };

    lsp_client.send(LspMessage::DidChange {
        uri: doc.uri.clone(),
        version,
        changes: vec![change],
    });

    trace!("[LSP] DidChange sent, version={}", version);
}

/// Core action execution - shared between LSP and non-LSP builds.
///
/// `buf` bundles the six per-entity buffer refs; taking it by value (the
/// fields are `&mut`s, so moving the bundle moves the borrows) lets us
/// destructure into individual `&mut` bindings the body uses verbatim.
/// Helpers below this dispatcher keep their individual-`&mut` signatures.
fn execute_action_core(
    buf: EditorBuf<'_>,
    action: EditorAction,
    indentation: &IndentationSettings,
    goto_line_state: &mut GotoLineState,
    fold_state: &mut FoldState,
) -> ActionResult {
    let EditorBuf {
        sel,
        hist,
        syntax,
        display,
        cursor,
        tv,
    } = buf;
    let mut result = ActionResult {
        text_changed: false,
        horizontal_move: false,
    };

    match action {
        EditorAction::InsertNewline => {
            insert_char(sel, hist, syntax, display, cursor, tv, '\n');
            result.text_changed = true;
        }
        EditorAction::InsertTab => {
            for _ in 0..indentation.tab_width {
                insert_char(sel, hist, syntax, display, cursor, tv, ' ');
            }
            result.text_changed = true;
        }

        EditorAction::DeleteBackward => {
            if sel.selection_start.is_some() {
                delete_selection(sel, hist, syntax, display, cursor, tv);
            } else {
                hist.delete_backward(sel, syntax, display, cursor, tv);
            }
            result.text_changed = true;
        }
        EditorAction::DeleteForward => {
            if sel.selection_start.is_some() {
                delete_selection(sel, hist, syntax, display, cursor, tv);
            } else {
                hist.delete_forward(sel, syntax, display, cursor, tv);
            }
            result.text_changed = true;
        }
        EditorAction::DeleteWordBackward => {
            if sel.selection_start.is_some() {
                delete_selection(sel, hist, syntax, display, cursor, tv);
            } else {
                delete_word_backward(sel, hist, cursor, tv);
            }
            result.text_changed = true;
        }
        EditorAction::DeleteWordForward => {
            if sel.selection_start.is_some() {
                delete_selection(sel, hist, syntax, display, cursor, tv);
            } else {
                delete_word_forward(sel, hist, cursor, tv);
            }
            result.text_changed = true;
        }
        EditorAction::DeleteLine => {
            // TODO: Implement line deletion
        }

        EditorAction::MoveCursorLeft => {
            sel.selection_start = None;
            sel.selection_end = None;
            move_cursor(cursor, &tv.rope, -1);
            sel.sync_cursors_from_primary(cursor);
            result.horizontal_move = true;
        }
        EditorAction::MoveCursorRight => {
            sel.selection_start = None;
            sel.selection_end = None;
            move_cursor(cursor, &tv.rope, 1);
            sel.sync_cursors_from_primary(cursor);
            result.horizontal_move = true;
        }
        EditorAction::MoveCursorUp => {
            sel.selection_start = None;
            sel.selection_end = None;
            move_cursor_up(cursor, &tv.rope);
            sel.sync_cursors_from_primary(cursor);
        }
        EditorAction::MoveCursorDown => {
            sel.selection_start = None;
            sel.selection_end = None;
            move_cursor_down(cursor, &tv.rope);
            sel.sync_cursors_from_primary(cursor);
        }
        EditorAction::MoveCursorWordLeft => {
            sel.selection_start = None;
            sel.selection_end = None;
            move_cursor_word_left(cursor, &tv.rope);
            sel.sync_cursors_from_primary(cursor);
            result.horizontal_move = true;
        }
        EditorAction::MoveCursorWordRight => {
            sel.selection_start = None;
            sel.selection_end = None;
            move_cursor_word_right(cursor, &tv.rope);
            sel.sync_cursors_from_primary(cursor);
            result.horizontal_move = true;
        }
        EditorAction::MoveCursorLineStart => {
            sel.selection_start = None;
            sel.selection_end = None;
            move_cursor_line_start(cursor, &tv.rope);
            sel.sync_cursors_from_primary(cursor);
        }
        EditorAction::MoveCursorLineEnd => {
            sel.selection_start = None;
            sel.selection_end = None;
            move_cursor_line_end(cursor, &tv.rope);
            sel.sync_cursors_from_primary(cursor);
        }
        EditorAction::MoveCursorDocumentStart => {
            sel.selection_start = None;
            sel.selection_end = None;
            cursor.cursor_pos = 0;
            sel.sync_cursors_from_primary(cursor);
        }
        EditorAction::MoveCursorDocumentEnd => {
            sel.selection_start = None;
            sel.selection_end = None;
            cursor.cursor_pos = tv.rope.len_chars();
            sel.sync_cursors_from_primary(cursor);
        }
        EditorAction::MoveCursorPageUp => {
            sel.selection_start = None;
            sel.selection_end = None;
            // TODO: Implement page up
            sel.sync_cursors_from_primary(cursor);
        }
        EditorAction::MoveCursorPageDown => {
            sel.selection_start = None;
            sel.selection_end = None;
            // TODO: Implement page down
            sel.sync_cursors_from_primary(cursor);
        }

        EditorAction::SelectLeft => {
            init_selection(sel, cursor);
            move_cursor(cursor, &tv.rope, -1);
            sel.selection_end = Some(cursor.cursor_pos);
            sel.sync_cursors_from_primary(cursor);
        }
        EditorAction::SelectRight => {
            init_selection(sel, cursor);
            move_cursor(cursor, &tv.rope, 1);
            sel.selection_end = Some(cursor.cursor_pos);
            sel.sync_cursors_from_primary(cursor);
        }
        EditorAction::SelectUp => {
            init_selection(sel, cursor);
            move_cursor_up(cursor, &tv.rope);
            sel.selection_end = Some(cursor.cursor_pos);
            sel.sync_cursors_from_primary(cursor);
        }
        EditorAction::SelectDown => {
            init_selection(sel, cursor);
            move_cursor_down(cursor, &tv.rope);
            sel.selection_end = Some(cursor.cursor_pos);
            sel.sync_cursors_from_primary(cursor);
        }
        EditorAction::SelectWordLeft => {
            init_selection(sel, cursor);
            move_cursor_word_left(cursor, &tv.rope);
            sel.selection_end = Some(cursor.cursor_pos);
            sel.sync_cursors_from_primary(cursor);
        }
        EditorAction::SelectWordRight => {
            init_selection(sel, cursor);
            move_cursor_word_right(cursor, &tv.rope);
            sel.selection_end = Some(cursor.cursor_pos);
            sel.sync_cursors_from_primary(cursor);
        }
        EditorAction::SelectLineStart => {
            init_selection(sel, cursor);
            move_cursor_line_start(cursor, &tv.rope);
            sel.selection_end = Some(cursor.cursor_pos);
            sel.sync_cursors_from_primary(cursor);
        }
        EditorAction::SelectLineEnd => {
            init_selection(sel, cursor);
            move_cursor_line_end(cursor, &tv.rope);
            sel.selection_end = Some(cursor.cursor_pos);
            sel.sync_cursors_from_primary(cursor);
        }
        EditorAction::SelectAll => {
            sel.selection_start = Some(0);
            sel.selection_end = Some(tv.rope.len_chars());
            cursor.cursor_pos = tv.rope.len_chars();
            sel.sync_cursors_from_primary(cursor);
        }
        EditorAction::ClearSelection => {
            sel.selection_start = None;
            sel.selection_end = None;
            sel.sync_cursors_from_primary(cursor);
        }

        EditorAction::Copy => {
            if let (Some(s), Some(e)) = (sel.selection_start, sel.selection_end) {
                let (start, end) = if s < e { (s, e) } else { (e, s) };
                let start = start.min(tv.rope.len_chars());
                let end = end.min(tv.rope.len_chars());
                let text = tv.rope.slice(start..end).to_string();
                if let Ok(mut clipboard) = Clipboard::new() {
                    let _ = clipboard.set_text(text);
                }
            }
        }
        EditorAction::Cut => {
            if let (Some(s), Some(e)) = (sel.selection_start, sel.selection_end) {
                let (start, end) = if s < e { (s, e) } else { (e, s) };
                let start = start.min(tv.rope.len_chars());
                let end = end.min(tv.rope.len_chars());
                let selected_text = tv.rope.slice(start..end).to_string();
                let cursor_before = cursor.cursor_pos;

                // Copy to clipboard
                if let Ok(mut clipboard) = Clipboard::new() {
                    let _ = clipboard.set_text(selected_text.clone());
                }

                // Delete the selection
                let start_byte = tv.rope.char_to_byte(start);
                let end_byte = tv.rope.char_to_byte(end);

                tv.rope.remove(start_byte..end_byte);
                cursor.cursor_pos = start;

                // Record for undo
                hist.history.record(EditOperation {
                    removed_text: selected_text,
                    inserted_text: String::new(),
                    position: start,
                    cursor_before,
                    cursor_after: start,
                    kind: EditKind::Other, // Cut is its own transaction
                });

                sel.selection_start = None;
                sel.selection_end = None;
                tv.content_version += 1;


                result.text_changed = true;
            }
        }
        EditorAction::Paste => {
            {
                if let Ok(mut clipboard) = Clipboard::new() {
                    if let Ok(text) = clipboard.get_text() {
                        let cursor_before = cursor.cursor_pos;
                        let mut deleted_text = String::new();
                        let paste_position;

                        // Delete selection if any
                        if let (Some(start), Some(end)) = (sel.selection_start, sel.selection_end) {
                            let (start, end) = if start < end {
                                (start, end)
                            } else {
                                (end, start)
                            };
                            let start = start.min(tv.rope.len_chars());
                            let end = end.min(tv.rope.len_chars());

                            deleted_text = tv.rope.slice(start..end).to_string();

                            let start_byte = tv.rope.char_to_byte(start);
                            let end_byte = tv.rope.char_to_byte(end);

                            tv.rope.remove(start_byte..end_byte);
                            cursor.cursor_pos = start;
                            sel.selection_start = None;
                            sel.selection_end = None;
                            paste_position = start;
                        } else {
                            paste_position = cursor.cursor_pos.min(tv.rope.len_chars());
                        }

                        // Insert pasted text

                        tv.rope.insert(paste_position, &text);
                        cursor.cursor_pos = paste_position + text.chars().count();
                        tv.content_version += 1;

                        // Record for undo (combined delete selection + insert paste)
                        hist.history.record(EditOperation {
                            removed_text: deleted_text,
                            inserted_text: text.clone(),
                            position: paste_position,
                            cursor_before,
                            cursor_after: cursor.cursor_pos,
                            kind: EditKind::Paste, // Paste is always its own transaction
                        });


                        result.text_changed = true;
                    }
                }
            }
        }

        EditorAction::Undo => {
            if hist.undo(syntax, display, cursor, tv) {
                result.text_changed = true;
            }
        }
        EditorAction::Redo => {
            if hist.redo(syntax, display, cursor, tv) {
                result.text_changed = true;
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
            sel.sync_cursors_from_primary(cursor);
            add_cursor_at_next_occurrence(sel, cursor, tv);
        }
        EditorAction::AddCursorAbove => {
            // Add cursor on the line above
            sel.sync_cursors_from_primary(cursor);
            add_cursor_above(sel, cursor, tv);
        }
        EditorAction::AddCursorBelow => {
            // Add cursor on the line below
            sel.sync_cursors_from_primary(cursor);
            add_cursor_below(sel, cursor, tv);
        }
        EditorAction::ClearSecondaryCursors => {
            // Clear all but primary cursor
            if sel.has_multiple_cursors(cursor) {
                sel.clear_secondary_cursors(cursor, tv);
            }
        }

        // Code folding actions
        EditorAction::ToggleFold => {
            let line = tv.rope.char_to_line(cursor.cursor_pos);
            fold_state.toggle_fold_at_line(line);
        }
        EditorAction::Fold => {
            let line = tv.rope.char_to_line(cursor.cursor_pos);
            fold_state.fold_at_line(line);
        }
        EditorAction::Unfold => {
            let line = tv.rope.char_to_line(cursor.cursor_pos);
            fold_state.unfold_at_line(line);
        }
        EditorAction::FoldAll => {
            fold_state.fold_all();
        }
        EditorAction::UnfoldAll => {
            fold_state.unfold_all();
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
fn add_cursor_above(sel: &mut SelectionState, cursor: &mut CursorState, tv: &mut TextViewState) {
    if cursor.cursors.is_empty() {
        return;
    }

    let primary_pos = cursor.cursors[0].position;
    let line_idx = tv.rope.char_to_line(primary_pos);

    if line_idx == 0 {
        return;
    }

    let line_start = tv.rope.line_to_char(line_idx);
    let col_offset = primary_pos - line_start;

    let prev_line_start = tv.rope.line_to_char(line_idx - 1);
    let prev_line_len = tv.rope.line(line_idx - 1).len_chars().saturating_sub(1);
    let new_pos = prev_line_start + col_offset.min(prev_line_len);

    sel.add_cursor(cursor, tv, new_pos);
}

/// Add a cursor on the line below the primary cursor
fn add_cursor_below(sel: &mut SelectionState, cursor: &mut CursorState, tv: &mut TextViewState) {
    if cursor.cursors.is_empty() {
        return;
    }

    let primary_pos = cursor.cursors[0].position;
    let line_idx = tv.rope.char_to_line(primary_pos);

    if line_idx + 1 >= tv.rope.len_lines() {
        return;
    }

    let line_start = tv.rope.line_to_char(line_idx);
    let col_offset = primary_pos - line_start;

    let next_line_start = tv.rope.line_to_char(line_idx + 1);
    let next_line_len = tv.rope.line(line_idx + 1).len_chars().saturating_sub(1);
    let new_pos = next_line_start + col_offset.min(next_line_len);

    sel.add_cursor(cursor, tv, new_pos);
}

/// Execute an editor action — unified entry point for all feature combinations.
pub fn execute_action(
    buf: EditorBuf<'_>,
    action: EditorAction,
    indentation: &IndentationSettings,
    goto_line_state: &mut GotoLineState,
    fold_state: &mut FoldState,
    #[cfg(feature = "lsp")] lsp_buf: LspBuf<'_>,
) {
    let EditorBuf {
        sel,
        hist,
        syntax,
        display,
        cursor,
        tv,
    } = buf;
    #[cfg(feature = "lsp")]
    let LspBuf {
        settings: lsp,
        client: lsp_client,
        completion: completion_state,
        document: mut lsp_document,
    } = lsp_buf;
    if action == EditorAction::ClearSelection {
        if sel.has_multiple_cursors(cursor) {
            sel.clear_secondary_cursors(cursor, tv);
            return;
        }
        if goto_line_state.active {
            goto_line_state.clear();
            return;
        }
    }

    // LSP completion navigation intercepts certain actions
    #[cfg(feature = "lsp")]
    {
        let filtered_count = completion_state.filtered_items().len();
        let max_visible = lsp.completion.max_items;

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
                    apply_completion(cursor, tv, completion_state);
                    send_did_change(&tv.rope, lsp_client, lsp_document.as_deref_mut());
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

        if action == EditorAction::RequestCompletion {
            request_completion(
                cursor,
                &tv.rope,
                lsp_client,
                completion_state,
                lsp_document.as_deref(),
            );
            return;
        }
    }

    let result = execute_action_core(
        EditorBuf {
            sel,
            hist,
            syntax,
            display,
            cursor,
            tv,
        },
        action,
        indentation,
        goto_line_state,
        fold_state,
    );

    #[cfg(feature = "lsp")]
    {
        if result.horizontal_move {
            completion_state.visible = false;
        }

        if action == EditorAction::DeleteBackward && completion_state.visible {
            if cursor.cursor_pos > completion_state.start_char_index {
                update_completion_filter(cursor, &tv.rope, completion_state);
            } else if cursor.cursor_pos == completion_state.start_char_index {
                completion_state.filter.clear();
                completion_state.selected_index = 0;
            } else {
                completion_state.visible = false;
                completion_state.filter.clear();
            }
        }

        if result.text_changed {
            send_did_change(&tv.rope, lsp_client, lsp_document.as_deref_mut());
        }
    }

    #[cfg(not(feature = "lsp"))]
    let _ = result;
}
