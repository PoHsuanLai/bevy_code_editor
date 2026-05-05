//! Text editing operations on [`EditHistoryState`].
//!
//! Insert, delete, undo / redo, set_text, anchor management. Methods mutate
//! the rope on the entity's [`bevy_text_engine::TextViewState`], the cursor
//! / selection components, and bookkeeping fields on `EditHistoryState`.
//!
//! Editor-level systems read [`EditHistoryState::pending_byte_edit`] (set by
//! every edit op for incremental tree-sitter reparse) and
//! [`EditHistoryState::invalidate_lines_from`] (set when an edit changes
//! line structure for entity-pool invalidation), then clear them.

use bevy_text_engine::TextViewState;
use ropey::Rope;

use crate::anchor::{Anchor, AnchorBias, TextEdit};
use crate::history::{EditKind, EditOperation};
use crate::selection::SelectionCollection;
use crate::state::{CursorState, EditHistoryState, SelectionState};

impl EditHistoryState {
    /// Record `(start_byte, old_end_byte, new_end_byte)` for the most recent edit.
    fn record_byte_edit(&mut self, start_byte: usize, old_end_byte: usize, new_end_byte: usize) {
        self.pending_byte_edit = Some((start_byte, old_end_byte, new_end_byte));
    }

    /// Insert character at cursor position (with undo recording).
    pub fn insert_char(
        &mut self,
        sel: &mut SelectionState,
        cursor: &mut CursorState,
        tv: &mut TextViewState,
        c: char,
    ) {
        self.insert_char_with_history(sel, cursor, tv, c, true);
    }

    /// Insert character with optional history recording.
    pub fn insert_char_with_history(
        &mut self,
        sel: &mut SelectionState,
        cursor: &mut CursorState,
        tv: &mut TextViewState,
        c: char,
        record_history: bool,
    ) {
        let cursor_pos = cursor.cursor_pos.min(tv.rope.len_chars());
        let cursor_before = cursor_pos;

        let start_byte = tv.rope.char_to_byte(cursor_pos);
        let char_byte_len = c.len_utf8();

        self.anchors.record_edit(TextEdit::insert(cursor_pos, 1));

        tv.rope.insert_char(cursor_pos, c);
        cursor.cursor_pos += 1;
        sel.apply_primary_cursor(cursor);
        tv.content_version += 1;

        self.record_byte_edit(start_byte, start_byte, start_byte + char_byte_len);

        if record_history {
            let kind = if c == '\n' {
                EditKind::Newline
            } else {
                EditKind::Insert
            };
            self.history.record(EditOperation {
                removed_text: String::new(),
                inserted_text: c.to_string(),
                position: cursor_before,
                cursor_before,
                cursor_after: cursor.cursor_pos,
                kind,
            });
        }

        if c == '\n' {
            let line_idx = tv.rope.char_to_line(cursor_pos);
            self.invalidate_lines_from = Some(line_idx);
        }
    }

    /// Delete character before cursor (with undo recording).
    pub fn delete_backward(
        &mut self,
        sel: &mut SelectionState,
        cursor: &mut CursorState,
        tv: &mut TextViewState,
    ) {
        self.delete_backward_with_history(sel, cursor, tv, true);
    }

    /// Delete character before cursor with optional history recording.
    pub fn delete_backward_with_history(
        &mut self,
        sel: &mut SelectionState,
        cursor: &mut CursorState,
        tv: &mut TextViewState,
        record_history: bool,
    ) {
        if cursor.cursor_pos > 0 && cursor.cursor_pos <= tv.rope.len_chars() {
            let cursor_before = cursor.cursor_pos;
            let line_idx = tv.rope.char_to_line(cursor.cursor_pos - 1);
            let deleted_char = tv.rope.char(cursor.cursor_pos - 1);
            let char_idx = tv.rope.char_to_byte(cursor.cursor_pos - 1);
            let byte_idx_end = tv.rope.char_to_byte(cursor.cursor_pos);

            self.anchors
                .record_edit(TextEdit::delete(cursor.cursor_pos - 1, cursor.cursor_pos));

            tv.rope.remove(char_idx..byte_idx_end);
            cursor.cursor_pos -= 1;
            sel.apply_primary_cursor(cursor);
            tv.content_version += 1;

            self.record_byte_edit(char_idx, byte_idx_end, char_idx);

            if record_history {
                self.history.record(EditOperation {
                    removed_text: deleted_char.to_string(),
                    inserted_text: String::new(),
                    position: cursor.cursor_pos,
                    cursor_before,
                    cursor_after: cursor.cursor_pos,
                    kind: EditKind::DeleteBackward,
                });
            }

            if deleted_char == '\n' {
                self.invalidate_lines_from = Some(line_idx);
            }
        }
    }

    /// Delete character after cursor (with undo recording).
    pub fn delete_forward(
        &mut self,
        sel: &mut SelectionState,
        cursor: &mut CursorState,
        tv: &mut TextViewState,
    ) {
        self.delete_forward_with_history(sel, cursor, tv, true);
    }

    /// Delete character after cursor with optional history recording.
    pub fn delete_forward_with_history(
        &mut self,
        sel: &mut SelectionState,
        cursor: &mut CursorState,
        tv: &mut TextViewState,
        record_history: bool,
    ) {
        if cursor.cursor_pos < tv.rope.len_chars() {
            let cursor_before = cursor.cursor_pos;
            let line_idx = tv.rope.char_to_line(cursor.cursor_pos);
            let deleted_char = tv.rope.char(cursor.cursor_pos);
            let char_idx = tv.rope.char_to_byte(cursor.cursor_pos);
            let byte_idx_end = tv.rope.char_to_byte(cursor.cursor_pos + 1);

            self.anchors
                .record_edit(TextEdit::delete(cursor.cursor_pos, cursor.cursor_pos + 1));

            tv.rope.remove(char_idx..byte_idx_end);
            sel.apply_primary_cursor(cursor);
            tv.content_version += 1;

            self.record_byte_edit(char_idx, byte_idx_end, char_idx);

            if record_history {
                self.history.record(EditOperation {
                    removed_text: deleted_char.to_string(),
                    inserted_text: String::new(),
                    position: cursor.cursor_pos,
                    cursor_before,
                    cursor_after: cursor.cursor_pos,
                    kind: EditKind::DeleteForward,
                });
            }

            if deleted_char == '\n' {
                self.invalidate_lines_from = Some(line_idx);
            }
        }
    }

    /// Insert text at a specific position (used for undo/redo).
    pub fn insert_text_at(&mut self, tv: &mut TextViewState, pos: usize, text: &str) {
        let pos = pos.min(tv.rope.len_chars());
        let text_char_len = text.chars().count();

        let start_byte = tv.rope.char_to_byte(pos);
        let text_byte_len = text.len();

        self.anchors
            .record_edit(TextEdit::insert(pos, text_char_len));

        tv.rope.insert(pos, text);
        tv.content_version += 1;

        if text.contains('\n') {
            let line_idx = tv.rope.char_to_line(pos);
            self.invalidate_lines_from = Some(line_idx);
        }

        self.record_byte_edit(start_byte, start_byte, start_byte + text_byte_len);
    }

    /// Remove text range (used for undo/redo).
    pub fn remove_range(&mut self, tv: &mut TextViewState, start: usize, end: usize) {
        let start = start.min(tv.rope.len_chars());
        let end = end.min(tv.rope.len_chars());
        if start < end {
            let start_byte = tv.rope.char_to_byte(start);
            let end_byte = tv.rope.char_to_byte(end);

            let removed_text: String = tv.rope.slice(start..end).chars().collect();
            let has_newlines = removed_text.contains('\n');

            self.anchors.record_edit(TextEdit::delete(start, end));

            tv.rope.remove(start_byte..end_byte);
            tv.content_version += 1;

            if has_newlines {
                let line_idx = tv.rope.char_to_line(start);
                self.invalidate_lines_from = Some(line_idx);
            }

            self.record_byte_edit(start_byte, end_byte, start_byte);
        }
    }

    /// Perform undo operation. Returns `true` if anything was undone.
    pub fn undo(
        &mut self,
        sel: &mut SelectionState,
        cursor: &mut CursorState,
        tv: &mut TextViewState,
    ) -> bool {
        if let Some(transaction) = self.history.pop_undo() {
            for op in transaction.operations.iter().rev() {
                if !op.inserted_text.is_empty() {
                    let end_pos = op.position + op.inserted_text.chars().count();
                    self.remove_range(tv, op.position, end_pos);
                }
                if !op.removed_text.is_empty() {
                    self.insert_text_at(tv, op.position, &op.removed_text);
                }
            }

            if let Some(first_op) = transaction.operations.first() {
                cursor.cursor_pos = first_op.cursor_before;
                sel.apply_primary_cursor(cursor);
            }

            self.history.push_redo(transaction);
            true
        } else {
            false
        }
    }

    /// Perform redo operation. Returns `true` if anything was redone.
    pub fn redo(
        &mut self,
        sel: &mut SelectionState,
        cursor: &mut CursorState,
        tv: &mut TextViewState,
    ) -> bool {
        if let Some(transaction) = self.history.pop_redo() {
            for op in transaction.operations.iter() {
                if !op.removed_text.is_empty() {
                    let end_pos = op.position + op.removed_text.chars().count();
                    self.remove_range(tv, op.position, end_pos);
                }
                if !op.inserted_text.is_empty() {
                    self.insert_text_at(tv, op.position, &op.inserted_text);
                }
            }

            if let Some(last_op) = transaction.operations.last() {
                cursor.cursor_pos = last_op.cursor_after;
                sel.apply_primary_cursor(cursor);
            }

            self.history.push_undo(transaction);
            true
        } else {
            false
        }
    }

    /// Replace all buffer text with `text`. Resets selection to a single
    /// cursor (clamped) and clears the anchor set.
    pub fn set_text(
        &mut self,
        sel: &mut SelectionState,
        cursor: &mut CursorState,
        tv: &mut TextViewState,
        text: &str,
    ) {
        let old_byte_len = tv.rope.len_bytes();
        let new_byte_len = text.len();

        tv.rope = Rope::from_str(text);
        cursor.cursor_pos = cursor.cursor_pos.min(tv.rope.len_chars());
        tv.content_version += 1;
        self.anchors.clear();
        sel.selections = SelectionCollection::with_cursor(cursor.cursor_pos);
        tv.max_content_width = 0.0;
        tv.max_width_line = None;

        self.record_byte_edit(0, old_byte_len, new_byte_len);
    }

    /// Create an anchor at the given position.
    pub fn create_anchor(&mut self, rope: &Rope, offset: usize, bias: AnchorBias) -> Anchor {
        let offset = offset.min(rope.len_chars());
        self.anchors.anchor_at(offset, bias)
    }

    /// Create an anchor with left bias.
    pub fn anchor_at(&mut self, rope: &Rope, offset: usize) -> Anchor {
        self.create_anchor(rope, offset, AnchorBias::Left)
    }

    /// Resolve an anchor's current position.
    pub fn resolve_anchor(&self, rope: &Rope, anchor: &Anchor) -> usize {
        self.anchors.resolve(anchor).min(rope.len_chars())
    }

    /// Apply pending anchor edits.
    pub fn apply_anchor_edits(&mut self) {
        self.anchors.apply_pending_edits();
    }

    /// Remove an anchor by its ID.
    pub fn remove_anchor(&mut self, id: u64) -> Option<Anchor> {
        self.anchors.remove(id)
    }
}

// ============================================================================
// SelectionState helpers — primary-cursor sync, multi-cursor management.
// ============================================================================

impl SelectionState {
    /// Push the imperative `cursor.cursor_pos` (mutated by movement and edit
    /// helpers) into the primary `Selection` in the collection. Drops any
    /// secondary cursors and collapses the selection — handlers that want to
    /// preserve a selection should use [`Self::apply_primary_with_anchor`].
    pub fn apply_primary_cursor(&mut self, cursor: &CursorState) {
        self.selections.set_cursor(cursor.cursor_pos);
    }

    /// Like [`Self::apply_primary_cursor`] but preserves an existing selection
    /// anchor (or, if there is no selection, uses `default_anchor` as the
    /// new anchor — typically the cursor position from before the move).
    pub fn apply_primary_with_anchor(&mut self, cursor: &CursorState, default_anchor: usize) {
        let anchor = if self.selections.primary().has_selection() {
            self.selections.primary().anchor_offset()
        } else {
            default_anchor
        };
        self.selections.set_selection(cursor.cursor_pos, anchor);
    }

    /// Read back the primary cursor's head into `cursor.cursor_pos` so the
    /// imperative-update helpers see the up-to-date offset.
    pub fn refresh_primary_cursor(&self, cursor: &mut CursorState) {
        cursor.cursor_pos = self.selections.primary().head_offset();
    }

    /// Add a new cursor at the given position (clamped to the rope length).
    pub fn add_cursor_at(&mut self, tv: &TextViewState, position: usize) {
        let position = position.min(tv.rope.len_chars());
        self.selections.add_cursor(position);
    }

    /// Add a new cursor with a selection range (both endpoints clamped).
    pub fn add_cursor_with_range(&mut self, tv: &TextViewState, head: usize, anchor: usize) {
        let head = head.min(tv.rope.len_chars());
        let anchor = anchor.min(tv.rope.len_chars());
        self.selections.add_selection_range(head, anchor);
    }

    /// Remove all cursors except the primary, then refresh `cursor.cursor_pos`.
    pub fn clear_secondary_cursors(&mut self, cursor: &mut CursorState) {
        self.selections.clear_secondary();
        self.refresh_primary_cursor(cursor);
    }

    /// Whether the collection currently holds more than one cursor / selection.
    pub fn has_multiple_cursors(&self) -> bool {
        self.selections.is_multiple()
    }

    /// Number of cursors / selections currently active.
    pub fn cursor_count(&self) -> usize {
        self.selections.len()
    }

    /// Apply pending edits to the selection collection.
    pub fn apply_selection_edits(&mut self) {
        self.selections.apply_pending_edits();
    }

    /// Get the primary selection from the collection.
    pub fn primary_selection(&self) -> &crate::selection::Selection {
        self.selections.primary()
    }

    /// Get all selection ranges as (start, end) tuples.
    pub fn selection_ranges(&self) -> Vec<(usize, usize)> {
        self.selections.ranges()
    }

    /// Range of the primary selection if non-empty (otherwise `None`).
    pub fn primary_range(&self) -> Option<(usize, usize)> {
        let primary = self.selections.primary();
        if primary.has_selection() {
            Some(primary.range())
        } else {
            None
        }
    }
}
