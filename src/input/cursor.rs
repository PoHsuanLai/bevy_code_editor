//! Cursor movement and word boundary helpers

use crate::types::*;

/// Initialize selection if not already started
pub fn init_selection(state: &mut CodeEditorState) {
    if state.selection_start.is_none() {
        state.selection_start = Some(state.cursor_pos);
        state.selection_end = Some(state.cursor_pos);
    }
}

/// Move cursor up one line
pub fn move_cursor_up(state: &mut CodeEditorState) {
    if state.cursor_pos > 0 {
        let line_idx = state.rope.char_to_line(state.cursor_pos);
        if line_idx > 0 {
            let line_start = state.rope.line_to_char(line_idx);
            let col_offset = state.cursor_pos - line_start;
            let prev_line_start = state.rope.line_to_char(line_idx - 1);
            let prev_line_len = state.rope.line(line_idx - 1).len_chars();
            state.cursor_pos = prev_line_start + col_offset.min(prev_line_len.saturating_sub(1));
        }
    }
}

/// Move cursor down one line
pub fn move_cursor_down(state: &mut CodeEditorState) {
    let line_idx = state.rope.char_to_line(state.cursor_pos);
    if line_idx + 1 < state.rope.len_lines() {
        let line_start = state.rope.line_to_char(line_idx);
        let col_offset = state.cursor_pos - line_start;
        let next_line_start = state.rope.line_to_char(line_idx + 1);
        let next_line_len = state.rope.line(line_idx + 1).len_chars();
        state.cursor_pos = next_line_start + col_offset.min(next_line_len.saturating_sub(1));
    }
}

/// Move cursor to line start
pub fn move_cursor_line_start(state: &mut CodeEditorState) {
    let line_idx = state.rope.char_to_line(state.cursor_pos);
    state.cursor_pos = state.rope.line_to_char(line_idx);
}

/// Move cursor to line end
pub fn move_cursor_line_end(state: &mut CodeEditorState) {
    let line_idx = state.rope.char_to_line(state.cursor_pos);
    let line_start = state.rope.line_to_char(line_idx);
    let line_len = state.rope.line(line_idx).len_chars();
    state.cursor_pos = line_start + line_len.saturating_sub(1).max(0);
}

/// Character classification for word boundary detection
#[derive(PartialEq, Eq, Clone, Copy)]
enum CharClass {
    Whitespace,
    Word,       // alphanumeric or underscore
    Punctuation,
}

fn classify_char(c: char) -> CharClass {
    if c.is_whitespace() {
        CharClass::Whitespace
    } else if c.is_alphanumeric() || c == '_' {
        CharClass::Word
    } else {
        CharClass::Punctuation
    }
}

/// Find the start of the previous word (for Ctrl+Left and Ctrl+Backspace)
/// This matches VSCode/Zed behavior: skip whitespace, then skip word characters
pub fn find_word_boundary_left(rope: &ropey::Rope, pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }

    let mut current = pos;

    // Skip any whitespace immediately before cursor
    while current > 0 {
        let c = rope.char(current - 1);
        if c.is_whitespace() && c != '\n' {
            current -= 1;
        } else {
            break;
        }
    }

    // If we hit a newline or start of document, stop
    if current == 0 {
        return 0;
    }

    // Determine the class of the character we're about to skip
    let class = classify_char(rope.char(current - 1));

    // Skip characters of the same class
    while current > 0 {
        let c = rope.char(current - 1);
        if c == '\n' {
            // Stop at line boundaries
            break;
        }
        if classify_char(c) == class {
            current -= 1;
        } else {
            break;
        }
    }

    current
}

/// Find the end of the next word (for Ctrl+Right and Ctrl+Delete)
/// This matches VSCode/Zed behavior: skip current word, then skip whitespace
pub fn find_word_boundary_right(rope: &ropey::Rope, pos: usize) -> usize {
    let len = rope.len_chars();
    if pos >= len {
        return len;
    }

    let mut current = pos;

    // Determine the class of the character at cursor
    let c = rope.char(current);

    // If we're on whitespace, skip it first
    if c.is_whitespace() {
        while current < len {
            let c = rope.char(current);
            if c == '\n' {
                // Move past the newline and stop
                current += 1;
                return current.min(len);
            }
            if c.is_whitespace() {
                current += 1;
            } else {
                break;
            }
        }
        return current;
    }

    // Skip characters of the same class
    let class = classify_char(c);
    while current < len {
        let c = rope.char(current);
        if c == '\n' {
            break;
        }
        if classify_char(c) == class {
            current += 1;
        } else {
            break;
        }
    }

    // Skip any trailing whitespace (but not newlines)
    while current < len {
        let c = rope.char(current);
        if c.is_whitespace() && c != '\n' {
            current += 1;
        } else {
            break;
        }
    }

    current
}

/// Move cursor to the previous word boundary
pub fn move_cursor_word_left(state: &mut CodeEditorState) {
    state.cursor_pos = find_word_boundary_left(&state.rope, state.cursor_pos);
}

/// Move cursor to the next word boundary
pub fn move_cursor_word_right(state: &mut CodeEditorState) {
    state.cursor_pos = find_word_boundary_right(&state.rope, state.cursor_pos);
}

/// Delete from cursor to previous word boundary
pub fn delete_word_backward(state: &mut CodeEditorState) {
    let cursor_before = state.cursor_pos;
    let word_start = find_word_boundary_left(&state.rope, state.cursor_pos);

    if word_start < cursor_before {
        // Get the text being deleted for undo
        let deleted_text: String = state.rope.slice(word_start..cursor_before).chars().collect();

        // Remove the text
        let start_byte = state.rope.char_to_byte(word_start);
        let end_byte = state.rope.char_to_byte(cursor_before);

        // Record edit for incremental parsing
        #[cfg(feature = "tree-sitter")]
        state.record_edit(start_byte, end_byte, start_byte);

        state.rope.remove(start_byte..end_byte);

        // Update cursor
        state.cursor_pos = word_start;

        // Record for undo
        state.history.record(EditOperation {
            removed_text: deleted_text,
            inserted_text: String::new(),
            position: word_start,
            cursor_before,
            cursor_after: word_start,
            kind: EditKind::DeleteBackward,
        });

        // Mark for update
        state.pending_update = true;
        let line_idx = state.rope.char_to_line(word_start);
        let new_line_count = state.rope.len_lines();
        state.dirty_lines = Some(line_idx..new_line_count);
        state.previous_line_count = new_line_count;
    }
}

/// Delete from cursor to next word boundary
pub fn delete_word_forward(state: &mut CodeEditorState) {
    let cursor_before = state.cursor_pos;
    let word_end = find_word_boundary_right(&state.rope, state.cursor_pos);

    if word_end > cursor_before {
        // Get the text being deleted for undo
        let deleted_text: String = state.rope.slice(cursor_before..word_end).chars().collect();

        // Remove the text
        let start_byte = state.rope.char_to_byte(cursor_before);
        let end_byte = state.rope.char_to_byte(word_end);

        // Record edit for incremental parsing
        #[cfg(feature = "tree-sitter")]
        state.record_edit(start_byte, end_byte, start_byte);

        state.rope.remove(start_byte..end_byte);

        // Cursor stays at the same position

        // Record for undo
        state.history.record(EditOperation {
            removed_text: deleted_text,
            inserted_text: String::new(),
            position: cursor_before,
            cursor_before,
            cursor_after: cursor_before,
            kind: EditKind::DeleteForward,
        });

        // Mark for update
        state.pending_update = true;
        let line_idx = state.rope.char_to_line(cursor_before);
        let new_line_count = state.rope.len_lines();
        state.dirty_lines = Some(line_idx..new_line_count);
        state.previous_line_count = new_line_count;
    }
}
