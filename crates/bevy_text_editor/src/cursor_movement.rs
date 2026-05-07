//! Cursor movement and word-boundary helpers.
//!
//! Pure rope/cursor functions — no syntax, no folding. Hosts that need
//! fold-aware movement (the code editor) layer their own movement on top.

use crate::state::CursorState;
use ropey::Rope;

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
    cursor.cursor_pos = line_start + line_len.saturating_sub(1);
}

/// Move cursor by a signed delta, clamped to the rope.
pub fn move_cursor(cursor: &mut CursorState, rope: &Rope, delta: isize) {
    if delta < 0 {
        let amount = (-delta) as usize;
        cursor.cursor_pos = cursor.cursor_pos.saturating_sub(amount);
    } else {
        let amount = delta as usize;
        cursor.cursor_pos = (cursor.cursor_pos + amount).min(rope.len_chars());
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum CharClass {
    Whitespace,
    Word,
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

/// Find the start of the previous word (Ctrl+Left, Ctrl+Backspace).
/// Skips trailing whitespace, then characters of the same class.
pub fn find_word_boundary_left(rope: &Rope, pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }

    let mut current = pos;

    while current > 0 {
        let c = rope.char(current - 1);
        if c.is_whitespace() && c != '\n' {
            current -= 1;
        } else {
            break;
        }
    }

    if current == 0 {
        return 0;
    }

    let class = classify_char(rope.char(current - 1));

    while current > 0 {
        let c = rope.char(current - 1);
        if c == '\n' {
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

/// Find the end of the next word (Ctrl+Right, Ctrl+Delete).
pub fn find_word_boundary_right(rope: &Rope, pos: usize) -> usize {
    let len = rope.len_chars();
    if pos >= len {
        return len;
    }

    let mut current = pos;

    let c = rope.char(current);

    if c.is_whitespace() {
        while current < len {
            let c = rope.char(current);
            if c == '\n' {
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

/// Move the cursor `lines` lines up (negative) or down (positive),
/// preserving the column offset like single-line up/down do. Used by
/// PageUp / PageDown.
pub fn move_cursor_lines(cursor: &mut CursorState, rope: &Rope, lines: isize) {
    if lines == 0 {
        return;
    }
    let line_idx = rope.char_to_line(cursor.cursor_pos);
    let line_start = rope.line_to_char(line_idx);
    let col_offset = cursor.cursor_pos - line_start;
    let last_line = rope.len_lines().saturating_sub(1);
    let target = if lines < 0 {
        line_idx.saturating_sub((-lines) as usize)
    } else {
        (line_idx + lines as usize).min(last_line)
    };
    let target_start = rope.line_to_char(target);
    let target_len = rope.line(target).len_chars();
    cursor.cursor_pos = target_start + col_offset.min(target_len.saturating_sub(1));
}
