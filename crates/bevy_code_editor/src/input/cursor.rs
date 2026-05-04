//! Cursor movement and word boundary helpers

use crate::text_view::TextViewState;
use crate::types::*;
use ropey::Rope;

pub fn init_selection(sel: &mut SelectionState, cursor: &CursorState) {
    if sel.selection_start.is_none() {
        sel.selection_start = Some(cursor.cursor_pos);
        sel.selection_end = Some(cursor.cursor_pos);
    }
}

pub fn move_cursor_up(cursor: &mut CursorState, rope: &Rope) {
    if cursor.cursor_pos > 0 {
        let line_idx = rope.char_to_line(cursor.cursor_pos);
        if line_idx > 0 {
            let line_start = rope.line_to_char(line_idx);
            let col_offset = cursor.cursor_pos - line_start;
            let prev_line_start = rope.line_to_char(line_idx - 1);
            let prev_line_len = rope.line(line_idx - 1).len_chars();
            cursor.cursor_pos = prev_line_start + col_offset.min(prev_line_len.saturating_sub(1));
        }
    }
}

pub fn move_cursor_down(cursor: &mut CursorState, rope: &Rope) {
    let line_idx = rope.char_to_line(cursor.cursor_pos);
    if line_idx + 1 < rope.len_lines() {
        let line_start = rope.line_to_char(line_idx);
        let col_offset = cursor.cursor_pos - line_start;
        let next_line_start = rope.line_to_char(line_idx + 1);
        let next_line_len = rope.line(line_idx + 1).len_chars();
        cursor.cursor_pos = next_line_start + col_offset.min(next_line_len.saturating_sub(1));
    }
}

pub fn move_cursor_line_start(cursor: &mut CursorState, rope: &Rope) {
    let line_idx = rope.char_to_line(cursor.cursor_pos);
    cursor.cursor_pos = rope.line_to_char(line_idx);
}

pub fn move_cursor_line_end(cursor: &mut CursorState, rope: &Rope) {
    let line_idx = rope.char_to_line(cursor.cursor_pos);
    let line_start = rope.line_to_char(line_idx);
    let line_len = rope.line(line_idx).len_chars();
    cursor.cursor_pos = line_start + line_len.saturating_sub(1).max(0);
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum CharClass {
    Whitespace,
    Word, // alphanumeric or underscore
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

pub fn move_cursor_word_left(cursor: &mut CursorState, rope: &Rope) {
    cursor.cursor_pos = find_word_boundary_left(rope, cursor.cursor_pos);
}

pub fn move_cursor_word_right(cursor: &mut CursorState, rope: &Rope) {
    cursor.cursor_pos = find_word_boundary_right(rope, cursor.cursor_pos);
}

pub fn delete_word_backward(
    state: &mut CodeEditorState,
    _sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    cursor: &mut CursorState,
    tv: &mut TextViewState,
) {
    let cursor_before = cursor.cursor_pos;
    let word_start = find_word_boundary_left(&tv.rope, cursor.cursor_pos);

    if word_start < cursor_before {
        let deleted_text: String = tv.rope.slice(word_start..cursor_before).chars().collect();

        // Remove the text
        let start_byte = tv.rope.char_to_byte(word_start);
        let end_byte = tv.rope.char_to_byte(cursor_before);

        // Record edit for incremental parsing
        #[cfg(feature = "tree-sitter")]
        state.record_edit(start_byte, end_byte, start_byte);

        tv.rope.remove(start_byte..end_byte);

        // Update cursor
        cursor.cursor_pos = word_start;

        // Record for undo
        hist.history.record(EditOperation {
            removed_text: deleted_text,
            inserted_text: String::new(),
            position: word_start,
            cursor_before,
            cursor_after: word_start,
            kind: EditKind::DeleteBackward,
        });

        // Mark for update
        tv.content_version += 1;
    }
}

/// Delete from cursor to next word boundary
pub fn delete_word_forward(
    state: &mut CodeEditorState,
    _sel: &mut SelectionState,
    hist: &mut EditHistoryState,
    cursor: &mut CursorState,
    tv: &mut TextViewState,
) {
    let cursor_before = cursor.cursor_pos;
    let word_end = find_word_boundary_right(&tv.rope, cursor.cursor_pos);

    if word_end > cursor_before {
        // Get the text being deleted for undo
        let deleted_text: String = tv.rope.slice(cursor_before..word_end).chars().collect();

        // Remove the text
        let start_byte = tv.rope.char_to_byte(cursor_before);
        let end_byte = tv.rope.char_to_byte(word_end);

        // Record edit for incremental parsing
        #[cfg(feature = "tree-sitter")]
        state.record_edit(start_byte, end_byte, start_byte);

        tv.rope.remove(start_byte..end_byte);

        // Cursor stays at the same position

        // Record for undo
        hist.history.record(EditOperation {
            removed_text: deleted_text,
            inserted_text: String::new(),
            position: cursor_before,
            cursor_before,
            cursor_after: cursor_before,
            kind: EditKind::DeleteForward,
        });

        // Mark for update
        tv.content_version += 1;
    }
}
