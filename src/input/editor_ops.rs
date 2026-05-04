//! Text search and cursor operations on CodeEditorState.

use crate::text_view::TextViewState;
use crate::types::*;
use ropey::Rope;

impl CodeEditorState {
    /// Get text content as string
    pub fn text(&self, rope: &Rope) -> String {
        rope.to_string()
    }

    /// Get line count
    pub fn line_count(&self, rope: &Rope) -> usize {
        rope.len_lines()
    }

    /// Move cursor by delta
    pub fn move_cursor(&self, cursor: &mut CursorState, rope: &Rope, delta: isize) {
        if delta < 0 {
            let amount = (-delta) as usize;
            cursor.cursor_pos = cursor.cursor_pos.saturating_sub(amount);
        } else {
            let amount = delta as usize;
            cursor.cursor_pos = (cursor.cursor_pos + amount).min(rope.len_chars());
        }
    }

    /// No-op stub kept for backwards compatibility.
    pub fn record_edit(&self, _start_byte: usize, _old_end_byte: usize, _new_end_byte: usize) {}

    /// Find word boundaries around a position and return (start, end)
    pub fn word_at_position(&self, rope: &Rope, pos: usize) -> Option<(usize, usize)> {
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
        &self,
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
        &self,
        sel: &mut SelectionState,
        cursor: &mut CursorState,
        tv: &mut TextViewState,
    ) -> bool {
        let search_text = if let Some(primary) = cursor.cursors.first() {
            if primary.has_selection() {
                let (start, end) = (primary.selection_start(), primary.selection_end());
                tv.rope.slice(start..end).to_string()
            } else {
                if let Some((start, end)) = self.word_at_position(&tv.rope, primary.position) {
                    cursor.cursors[0] = Cursor::with_selection(end, start);
                    sel.sync_primary_cursor(cursor);
                    tv.pending_update = true;
                    return true;
                }
                return false;
            }
        } else {
            return false;
        };

        if search_text.is_empty() {
            return false;
        }

        let search_from = cursor
            .cursors
            .iter()
            .map(|c| c.selection_end())
            .max()
            .unwrap_or(0);

        if let Some((start, end)) = self.find_next_occurrence(&tv.rope, &search_text, search_from) {
            let already_covered = cursor.cursors.iter().any(|c| {
                let (cs, ce) = (c.selection_start(), c.selection_end());
                start >= cs && end <= ce
            });

            if !already_covered {
                sel.add_cursor_with_selection(cursor, tv, end, start);
                return true;
            }
        }

        false
    }
}
