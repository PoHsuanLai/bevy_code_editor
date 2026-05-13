//! Text editing operations on [`EditHistoryState`].
//!
//! Insert, delete, undo / redo, set_text, anchor management. Methods mutate
//! the rope on the entity's [`bevy_instanced_text::TextBuffer<RopeBuffer>`], the cursor
//! / selection components, and bookkeeping fields on `EditHistoryState`.
//!
//! Editor-level systems read [`EditHistoryState::pending_byte_edit`] (set by
//! every edit op for incremental tree-sitter reparse) then clear it.

use bevy_instanced_text::{ContentMetrics, TextBuffer};
use crate::rope_content::RopeBuffer;
use ropey::Rope;

use bevy_text_interaction::{
    Anchor, AnchorBias, CursorState, SelectionCollection, SelectionState, TextEdit,
};
use crate::history::{EditKind, EditOperation};
use crate::state::{EditDelta, EditHistoryState, EditPoint};

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
    pub start: usize,
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
        buffer: &mut TextBuffer<RopeBuffer>,
        start_char: usize,
        end_char: usize,
        text: &str,
        kind: EditKind,
        record_history: bool,
    ) -> EditOutcome {
        let len = buffer.len_chars();
        let start = start_char.min(len);
        let end = end_char.min(len).max(start);

        let removed_text: String = if start < end {
            buffer.slice(start..end).chars().collect()
        } else {
            String::new()
        };
        let inserted_chars = text.chars().count();
        let inserted_bytes = text.len();

        let start_byte = buffer.char_to_byte(start);
        let end_byte = buffer.char_to_byte(end);
        let start_position = point_at_byte(buffer.rope(), start_byte);
        let old_end_position = point_at_byte(buffer.rope(), end_byte);

        // Capture pre-edit rope when an LSP-style consumer asked for it.
        // Ropey's structural sharing makes this O(log n); the snapshot is
        // dropped same-frame after the OnEdit observer chain runs.
        if self.snapshot_pre_edits && self.pre_edit_rope.is_none() {
            self.pre_edit_rope = Some(buffer.rope().clone());
        }

        if start < end {
            self.anchors.record_edit(TextEdit::delete(start, end));
        }
        if inserted_chars > 0 {
            self.anchors
                .record_edit(TextEdit::insert(start, inserted_chars));
        }

        if start < end {
            buffer.remove(start..end);
        }
        if !text.is_empty() {
            buffer.insert(start, text);
        }
        // Change detection: mutations through DerefMut already marked
        // TextBuffer<RopeBuffer> changed. No manual content_version bump needed.

        let new_end_byte = start_byte + inserted_bytes;
        let new_cursor_pos = start + inserted_chars;

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
            new_end_position: point_at_byte(buffer.rope(), new_end_byte),
        });

        EditOutcome {
            start,
            new_cursor_pos,
        }
    }

    pub fn insert_char(
        &mut self,
        sel: &mut SelectionState,
        cursor: &mut CursorState,
        buffer: &mut TextBuffer<RopeBuffer>,
        c: char,
    ) {
        let pos = cursor.cursor_pos.min(buffer.len_chars());
        let kind = if c == '\n' {
            EditKind::Newline
        } else {
            EditKind::Insert
        };
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
        buffer: &mut TextBuffer<RopeBuffer>,
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
        buffer: &mut TextBuffer<RopeBuffer>,
    ) {
        if cursor.cursor_pos >= buffer.len_chars() {
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
    pub fn insert_text_at(&mut self, buffer: &mut TextBuffer<RopeBuffer>, pos: usize, text: &str) {
        self.replace_range(buffer, pos, pos, text, EditKind::Other, false);
    }

    /// Remove text range (used for undo/redo). Skips history recording.
    pub fn remove_range(&mut self, buffer: &mut TextBuffer<RopeBuffer>, start: usize, end: usize) {
        self.replace_range(buffer, start, end, "", EditKind::Other, false);
    }

    /// Perform undo operation. Returns `true` if anything was undone.
    pub fn undo(
        &mut self,
        sel: &mut SelectionState,
        cursor: &mut CursorState,
        buffer: &mut TextBuffer<RopeBuffer>,
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
        buffer: &mut TextBuffer<RopeBuffer>,
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
        buffer: &mut TextBuffer<RopeBuffer>,
        metrics: &mut ContentMetrics,
        text: &str,
    ) {
        let old_len = buffer.len_chars();
        self.replace_range(buffer, 0, old_len, text, EditKind::Other, false);
        self.anchors.clear();
        cursor.cursor_pos = cursor.cursor_pos.min(buffer.len_chars());
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

// `SelectionState`'s `apply_primary_cursor`, `add_cursor_at`,
// `clear_secondary_cursors`, `primary_range`, etc. now live in
// `bevy_text_interaction::state` so terminals can use them too.
