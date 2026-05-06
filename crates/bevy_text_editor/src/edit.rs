//! Text editing operations on [`EditHistoryState`].
//!
//! Insert, delete, undo / redo, set_text, anchor management. Methods mutate
//! the rope on the entity's [`bevy_text_engine::TextBuffer`], the cursor
//! / selection components, and bookkeeping fields on `EditHistoryState`.
//!
//! Editor-level systems read [`EditHistoryState::pending_byte_edit`] (set by
//! every edit op for incremental tree-sitter reparse) and
//! [`EditHistoryState::invalidate_lines_from`] (set when an edit changes
//! line structure for entity-pool invalidation), then clear them.

use bevy_text_engine::{ContentMetrics, TextBuffer};
use ropey::Rope;

use crate::anchor::{Anchor, AnchorBias, TextEdit};
use crate::history::{EditKind, EditOperation};
use crate::selection::SelectionCollection;
use crate::state::{CursorState, EditDelta, EditHistoryState, EditPoint, SelectionState};

/// Compute (row, byte_column) for a given byte offset in `rope`.
pub fn point_at_byte(rope: &Rope, byte_offset: usize) -> EditPoint {
    let byte_offset = byte_offset.min(rope.len_bytes());
    let line = rope.byte_to_line(byte_offset);
    let line_start_byte = rope.line_to_byte(line);
    EditPoint {
        row: line as u32,
        column_byte: (byte_offset - line_start_byte) as u32,
    }
}

/// Outcome of a [`EditHistoryState::replace_range`] call. The cursor that the
/// editor wants to "follow" the edit lands at `new_cursor_pos`.
#[derive(Clone, Debug)]
pub struct EditOutcome {
    pub start_char: usize,
    pub new_cursor_pos: usize,
}

impl EditHistoryState {
    /// Replace `[start_char..end_char]` with `text`. The single primitive that
    /// every editor mutation funnels through — handles char-vs-byte ranges,
    /// position capture, anchor edits, history recording, content_version
    /// bumps, and `EditDelta` for downstream reparse / LSP.
    ///
    /// `kind` controls the [`EditKind`] recorded in undo history.
    /// `record_history = false` skips history (for undo/redo replay or
    /// programmatic edits that shouldn't push a new transaction).
    pub fn replace_range(
        &mut self,
        buffer: &mut TextBuffer,
        start_char: usize,
        end_char: usize,
        text: &str,
        kind: EditKind,
        record_history: bool,
    ) -> EditOutcome {
        let len = buffer.rope.len_chars();
        let start = start_char.min(len);
        let end = end_char.min(len).max(start);

        let removed_text: String = if start < end {
            buffer.rope.slice(start..end).chars().collect()
        } else {
            String::new()
        };
        let inserted_chars = text.chars().count();
        let inserted_bytes = text.len();

        let start_byte = buffer.rope.char_to_byte(start);
        let end_byte = buffer.rope.char_to_byte(end);
        let start_position = point_at_byte(&buffer.rope, start_byte);
        let old_end_position = point_at_byte(&buffer.rope, end_byte);

        // Capture pre-edit rope when an LSP-style consumer asked for it.
        // Ropey's structural sharing makes this O(log n); the snapshot is
        // dropped same-frame after the OnEdit observer chain runs.
        if self.snapshot_pre_edits && self.pre_edit_rope.is_none() {
            self.pre_edit_rope = Some(buffer.rope.clone());
        }

        if start < end {
            self.anchors.record_edit(TextEdit::delete(start, end));
        }
        if inserted_chars > 0 {
            self.anchors.record_edit(TextEdit::insert(start, inserted_chars));
        }

        if start < end {
            buffer.rope.remove(start..end);
        }
        if !text.is_empty() {
            buffer.rope.insert(start, text);
        }
        buffer.content_version += 1;

        let new_end_byte = start_byte + inserted_bytes;
        let new_cursor_pos = start + inserted_chars;

        if removed_text.contains('\n') || text.contains('\n') {
            self.invalidate_lines_from = Some(buffer.rope.char_to_line(start));
        }

        if record_history && (!removed_text.is_empty() || !text.is_empty()) {
            self.history.record(EditOperation {
                removed_text: removed_text.clone(),
                inserted_text: text.to_string(),
                position: start,
                cursor_before: start,
                cursor_after: new_cursor_pos,
                kind,
            });
        }

        self.pending_byte_edit = Some(EditDelta {
            start_byte,
            old_end_byte: end_byte,
            new_end_byte,
            start_position,
            old_end_position,
            new_end_position: point_at_byte(&buffer.rope, new_end_byte),
        });

        EditOutcome {
            start_char: start,
            new_cursor_pos,
        }
    }

    pub fn insert_char(
        &mut self,
        sel: &mut SelectionState,
        cursor: &mut CursorState,
        buffer: &mut TextBuffer,
        c: char,
    ) {
        let pos = cursor.cursor_pos.min(buffer.rope.len_chars());
        let kind = if c == '\n' { EditKind::Newline } else { EditKind::Insert };
        let mut buf = [0u8; 4];
        let s = c.encode_utf8(&mut buf);
        let outcome = self.replace_range(buffer, pos, pos, s, kind, true);
        cursor.cursor_pos = outcome.new_cursor_pos;
        sel.apply_primary_cursor(cursor);
    }

    pub fn delete_backward(
        &mut self,
        sel: &mut SelectionState,
        cursor: &mut CursorState,
        buffer: &mut TextBuffer,
    ) {
        if cursor.cursor_pos == 0 {
            return;
        }
        let outcome = self.replace_range(
            buffer,
            cursor.cursor_pos - 1,
            cursor.cursor_pos,
            "",
            EditKind::DeleteBackward,
            true,
        );
        cursor.cursor_pos = outcome.new_cursor_pos;
        sel.apply_primary_cursor(cursor);
    }

    pub fn delete_forward(
        &mut self,
        sel: &mut SelectionState,
        cursor: &mut CursorState,
        buffer: &mut TextBuffer,
    ) {
        if cursor.cursor_pos >= buffer.rope.len_chars() {
            return;
        }
        self.replace_range(
            buffer,
            cursor.cursor_pos,
            cursor.cursor_pos + 1,
            "",
            EditKind::DeleteForward,
            true,
        );
        sel.apply_primary_cursor(cursor);
    }

    /// Insert text at a specific position (used for undo/redo). Skips
    /// history recording — the caller already manages the transaction.
    pub fn insert_text_at(&mut self, buffer: &mut TextBuffer, pos: usize, text: &str) {
        self.replace_range(buffer, pos, pos, text, EditKind::Other, false);
    }

    /// Remove text range (used for undo/redo). Skips history recording.
    pub fn remove_range(&mut self, buffer: &mut TextBuffer, start: usize, end: usize) {
        self.replace_range(buffer, start, end, "", EditKind::Other, false);
    }

    /// Perform undo operation. Returns `true` if anything was undone.
    pub fn undo(
        &mut self,
        sel: &mut SelectionState,
        cursor: &mut CursorState,
        buffer: &mut TextBuffer,
    ) -> bool {
        if let Some(transaction) = self.history.pop_undo() {
            for op in transaction.operations.iter().rev() {
                if !op.inserted_text.is_empty() {
                    let end_pos = op.position + op.inserted_text.chars().count();
                    self.remove_range(buffer, op.position, end_pos);
                }
                if !op.removed_text.is_empty() {
                    self.insert_text_at(buffer, op.position, &op.removed_text);
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
        buffer: &mut TextBuffer,
    ) -> bool {
        if let Some(transaction) = self.history.pop_redo() {
            for op in transaction.operations.iter() {
                if !op.removed_text.is_empty() {
                    let end_pos = op.position + op.removed_text.chars().count();
                    self.remove_range(buffer, op.position, end_pos);
                }
                if !op.inserted_text.is_empty() {
                    self.insert_text_at(buffer, op.position, &op.inserted_text);
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
    /// cursor (clamped) and clears the anchor set. Skips history.
    pub fn set_text(
        &mut self,
        sel: &mut SelectionState,
        cursor: &mut CursorState,
        buffer: &mut TextBuffer,
        metrics: &mut ContentMetrics,
        text: &str,
    ) {
        let old_len = buffer.rope.len_chars();
        self.replace_range(buffer, 0, old_len, text, EditKind::Other, false);
        self.anchors.clear();
        cursor.cursor_pos = cursor.cursor_pos.min(buffer.rope.len_chars());
        sel.selections = SelectionCollection::with_cursor(cursor.cursor_pos);
        metrics.max_content_width = 0.0;
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
    pub fn add_cursor_at(&mut self, buffer: &TextBuffer, position: usize) {
        let position = position.min(buffer.rope.len_chars());
        self.selections.add_cursor(position);
    }

    /// Add a new cursor with a selection range (both endpoints clamped).
    pub fn add_cursor_with_range(&mut self, buffer: &TextBuffer, head: usize, anchor: usize) {
        let head = head.min(buffer.rope.len_chars());
        let anchor = anchor.min(buffer.rope.len_chars());
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
