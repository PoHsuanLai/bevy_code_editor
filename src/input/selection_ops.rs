//! Selection and multi-cursor operations on SelectionState.

use crate::text_view::TextViewState;
use crate::types::*;

impl SelectionState {
    // ========== Multi-cursor methods ==========

    /// Sync the primary cursor (cursor_pos/selection_start/selection_end) with cursors[0]
    pub fn sync_primary_cursor(&mut self, cursor: &mut CursorState) {
        if let Some(primary) = cursor.cursors.first() {
            cursor.cursor_pos = primary.position;
            self.selection_start = primary.anchor;
            self.selection_end = if primary.anchor.is_some() {
                Some(primary.position)
            } else {
                None
            };
        }
    }

    /// Sync cursors[0] from the primary cursor fields
    pub fn sync_cursors_from_primary(&mut self, cursor: &mut CursorState) {
        if cursor.cursors.is_empty() {
            cursor.cursors.push(Cursor::new(cursor.cursor_pos));
        }
        cursor.cursors[0].position = cursor.cursor_pos;
        cursor.cursors[0].anchor = self.selection_start;
    }

    /// Add a new cursor at the given position
    pub fn add_cursor(
        &mut self,
        cursor: &mut CursorState,
        tv: &mut TextViewState,
        position: usize,
    ) {
        let position = position.min(tv.rope.len_chars());
        if !cursor.cursors.iter().any(|c| c.position == position) {
            cursor.cursors.push(Cursor::new(position));
            self.sort_and_merge_cursors(cursor);
        }
    }

    /// Add a new cursor with selection
    pub fn add_cursor_with_selection(
        &mut self,
        cursor: &mut CursorState,
        tv: &mut TextViewState,
        position: usize,
        anchor: usize,
    ) {
        let position = position.min(tv.rope.len_chars());
        let anchor = anchor.min(tv.rope.len_chars());
        cursor
            .cursors
            .push(Cursor::with_selection(position, anchor));
        self.sort_and_merge_cursors(cursor);
    }

    /// Remove all cursors except the primary one
    pub fn clear_secondary_cursors(&mut self, cursor: &mut CursorState, _tv: &mut TextViewState) {
        if !cursor.cursors.is_empty() {
            cursor.cursors.truncate(1);
        }
        self.sync_primary_cursor(cursor);
    }

    /// Check if we have multiple cursors
    pub fn has_multiple_cursors(&self, cursor: &CursorState) -> bool {
        cursor.cursors.len() > 1
    }

    /// Get the number of cursors
    pub fn cursor_count(&self, cursor: &CursorState) -> usize {
        cursor.cursors.len()
    }

    /// Sort cursors by position and merge overlapping selections
    pub fn sort_and_merge_cursors(&mut self, cursor: &mut CursorState) {
        if cursor.cursors.len() <= 1 {
            return;
        }

        cursor.cursors.sort_by_key(|c| c.position);

        let mut merged: Vec<Cursor> = Vec::with_capacity(cursor.cursors.len());
        for c in cursor.cursors.drain(..) {
            if let Some(last) = merged.last_mut() {
                let last_end = last.selection_end();
                let cursor_start = c.selection_start();

                if cursor_start <= last_end {
                    let new_end = c.selection_end().max(last_end);
                    if last.anchor.is_some() || c.anchor.is_some() {
                        let new_start = last.selection_start().min(cursor_start);
                        last.anchor = Some(new_start);
                        last.position = new_end;
                    } else {
                        last.position = new_end;
                    }
                } else {
                    merged.push(c);
                }
            } else {
                merged.push(c);
            }
        }
        cursor.cursors = merged;

        self.sync_primary_cursor(cursor);
    }

    // ========== SelectionCollection methods ==========

    /// Apply pending edits to the selection collection
    pub fn apply_selection_edits(&mut self) {
        self.selections.apply_pending_edits();
    }

    /// Sync the legacy cursor fields from the SelectionCollection
    pub fn sync_from_selections(&mut self, cursor: &mut CursorState) {
        let primary = self.selections.primary();
        cursor.cursor_pos = primary.head_offset();
        if primary.has_selection() {
            self.selection_start = Some(primary.anchor_offset());
            self.selection_end = Some(primary.head_offset());
        } else {
            self.selection_start = None;
            self.selection_end = None;
        }
        cursor.cursors = self.selections.to_cursors();
    }

    /// Sync the SelectionCollection from legacy cursor fields
    pub fn sync_to_selections(&mut self, cursor: &CursorState) {
        if let Some(anchor) = self.selection_start {
            self.selections.set_selection(cursor.cursor_pos, anchor);
        } else {
            self.selections.set_cursor(cursor.cursor_pos);
        }
    }

    /// Get the primary selection from the collection
    pub fn primary_selection(&self) -> &Selection {
        self.selections.primary()
    }

    /// Get all selection ranges as (start, end) tuples
    pub fn selection_ranges(&self) -> Vec<(usize, usize)> {
        self.selections.ranges()
    }

    /// Check if there are multiple selections
    pub fn has_multiple_selections(&self) -> bool {
        self.selections.is_multiple()
    }

    /// Add a new selection at the given position (cursor only)
    pub fn add_selection(
        &mut self,
        cursor: &mut CursorState,
        tv: &mut TextViewState,
        offset: usize,
    ) {
        let offset = offset.min(tv.rope.len_chars());
        self.selections.add_cursor(offset);
        self.sync_from_selections(cursor);
    }

    /// Add a new selection with a range
    pub fn add_selection_range(
        &mut self,
        cursor: &mut CursorState,
        tv: &mut TextViewState,
        head: usize,
        anchor: usize,
    ) {
        let head = head.min(tv.rope.len_chars());
        let anchor = anchor.min(tv.rope.len_chars());
        self.selections.add_selection_range(head, anchor);
        self.sync_from_selections(cursor);
    }

    /// Clear all secondary selections, keeping only the primary
    pub fn clear_secondary_selections_sel(
        &mut self,
        cursor: &mut CursorState,
        _tv: &mut TextViewState,
    ) {
        self.selections.clear_secondary();
        self.sync_from_selections(cursor);
    }

    /// Move the primary selection to a new position
    pub fn set_primary_selection(
        &mut self,
        cursor: &mut CursorState,
        tv: &mut TextViewState,
        head: usize,
        extend: bool,
    ) {
        let head = head.min(tv.rope.len_chars());
        self.selections.move_primary(head, extend);
        self.sync_from_selections(cursor);
    }
}
