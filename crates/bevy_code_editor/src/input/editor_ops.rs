//! Text search and cursor operations on the editor's rope buffer.

use crate::text_view::TextViewState;
use crate::types::*;
use ropey::Rope;

/// Move cursor by delta
pub fn move_cursor(cursor: &mut CursorState, rope: &Rope, delta: isize) {
    if delta < 0 {
        let amount = (-delta) as usize;
        cursor.cursor_pos = cursor.cursor_pos.saturating_sub(amount);
    } else {
        let amount = delta as usize;
        cursor.cursor_pos = (cursor.cursor_pos + amount).min(rope.len_chars());
    }
}

/// Find word boundaries around a position and return (start, end)
pub fn word_at_position(rope: &Rope, pos: usize) -> Option<(usize, usize)> {
    let pos = pos.min(rope.len_chars());
    if pos >= rope.len_chars() {
        return None;
    }

    let c = rope.char(pos);
    if !c.is_alphanumeric() && c != '_' {
        return None;
    }

    let mut start = pos;
    while start > 0 {
        let prev = rope.char(start - 1);
        if prev.is_alphanumeric() || prev == '_' {
            start -= 1;
        } else {
            break;
        }
    }

    let mut end = pos;
    while end < rope.len_chars() {
        let ch = rope.char(end);
        if ch.is_alphanumeric() || ch == '_' {
            end += 1;
        } else {
            break;
        }
    }

    if start < end {
        Some((start, end))
    } else {
        None
    }
}

/// Find the next occurrence of text after a given position
pub fn find_next_occurrence(
    rope: &Rope,
    text: &str,
    after_pos: usize,
) -> Option<(usize, usize)> {
    if text.is_empty() {
        return None;
    }

    let text_chars: Vec<char> = text.chars().collect();
    let text_len = text_chars.len();
    let rope_len = rope.len_chars();

    // Search from after_pos to end
    let mut pos = after_pos;
    while pos + text_len <= rope_len {
        let mut matches = true;
        for (i, &tc) in text_chars.iter().enumerate() {
            if rope.char(pos + i) != tc {
                matches = false;
                break;
            }
        }
        if matches {
            return Some((pos, pos + text_len));
        }
        pos += 1;
    }

    // Wrap around and search from beginning to after_pos
    pos = 0;
    while pos + text_len <= after_pos && pos + text_len <= rope_len {
        let mut matches = true;
        for (i, &tc) in text_chars.iter().enumerate() {
            if rope.char(pos + i) != tc {
                matches = false;
                break;
            }
        }
        if matches {
            return Some((pos, pos + text_len));
        }
        pos += 1;
    }

    None
}

/// Add cursor at next occurrence of current selection/word (Ctrl+D behavior)
pub fn add_cursor_at_next_occurrence(
    sel: &mut SelectionState,
    cursor: &mut CursorState,
    tv: &mut TextViewState,
) -> bool {
    let primary = sel.selections.primary();
    let search_text = if primary.has_selection() {
        let (start, end) = primary.range();
        tv.rope.slice(start..end).to_string()
    } else if let Some((start, end)) = word_at_position(&tv.rope, primary.head_offset()) {
        // First Cmd+D on a bare cursor: select the word under the cursor.
        // Match the legacy behavior of placing the head at `end` (so the
        // caret sits at the end of the word) and the anchor at `start`.
        sel.selections.set_selection(end, start);
        sel.refresh_primary_cursor(cursor);
        return true;
    } else {
        return false;
    };

    if search_text.is_empty() {
        return false;
    }

    let search_from = sel
        .selections
        .iter()
        .map(|s| s.end())
        .max()
        .unwrap_or(0);

    if let Some((start, end)) = find_next_occurrence(&tv.rope, &search_text, search_from) {
        let already_covered = sel.selections.iter().any(|s| {
            let (cs, ce) = s.range();
            start >= cs && end <= ce
        });

        if !already_covered {
            sel.add_cursor_with_range(tv, end, start);
            sel.refresh_primary_cursor(cursor);
            return true;
        }
    }

    false
}
