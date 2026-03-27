//! Editor component types and state

use bevy::prelude::*;
use ropey::Rope;
use std::time::Instant;

use crate::text_view::TextViewState;

use super::anchor::{Anchor, AnchorBias, AnchorSet, TextEdit};
use super::display_map::{DisplayMap, HighlightedToken, LineSegment};
use super::history::{EditHistory, EditKind, EditOperation};
use super::selection::{Cursor, Selection, SelectionCollection};

/// Configuration for viewport behavior
#[derive(Resource, Clone, Copy, Debug)]
pub struct ViewportConfig {
    /// If true, viewport automatically resizes to match window size.
    /// If false, you must manually set ViewportDimensions.
    /// Default: true (for backward compatibility)
    pub auto_resize_to_window: bool,
}

impl Default for ViewportConfig {
    fn default() -> Self {
        Self {
            auto_resize_to_window: true,
        }
    }
}

/// Viewport dimensions and layout information
///
/// This resource tracks both the viewport size and the computed layout for rendering.
/// The UI plugin (or custom UI) is responsible for computing the layout based on
/// its own settings and updating this resource.
#[derive(Resource, Clone, Copy, Debug)]
pub struct ViewportDimensions {
    /// Viewport width in pixels
    pub width: u32,

    /// Viewport height in pixels
    pub height: u32,

    /// Horizontal offset for the editor content center (useful for sidebars/panels)
    /// This is the center X position in world coordinates
    pub offset_x: f32,

    /// Vertical offset for the editor content center (useful for panels)
    /// This is the center Y position in world coordinates
    pub offset_y: f32,

    // === Computed Layout (set by UI plugin) ===
    /// Left margin/padding before text starts
    pub text_area_left: f32,

    /// Top margin/padding before text starts
    pub text_area_top: f32,

    /// Width of the gutter area (line numbers, etc.)
    pub gutter_width: f32,

    /// X position of the separator line between gutter and code
    pub separator_x: f32,
}

impl ViewportDimensions {
    /// Calculate the world coordinate of the viewport's left edge
    pub fn world_left(&self) -> f32 {
        if self.offset_x == 0.0 && self.offset_y == 0.0 {
            // Auto-resize mode (default): viewport is centered at (0,0)
            -(self.width as f32) / 2.0
        } else {
            // Manual mode: offset_x is the left edge
            self.offset_x
        }
    }

    /// Calculate the world coordinate of the viewport's top edge
    pub fn world_top(&self) -> f32 {
        if self.offset_x == 0.0 && self.offset_y == 0.0 {
            // Auto-resize mode (default): viewport is centered at (0,0)
            self.height as f32 / 2.0
        } else {
            // Manual mode: offset_y is the top edge
            self.offset_y
        }
    }
}

impl Default for ViewportDimensions {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            offset_x: 0.0,
            offset_y: 0.0,
            // Default layout values (can be overridden by UI plugin)
            text_area_left: 80.0,
            text_area_top: 10.0,
            gutter_width: 60.0,
            separator_x: 70.0,
        }
    }
}

/// Marker component for the code editor entity.
///
/// The editor entity has `CodeEditor` + `CodeEditorState` + `CursorState` + `TextViewState` + `TextViewViewport`.
#[derive(Component, Default)]
pub struct CodeEditor;

/// Cursor state component — tracks cursor positions and multi-cursor state.
#[derive(Component)]
pub struct CursorState {
    /// Cursor position (char index) - primary cursor for backward compatibility
    pub cursor_pos: usize,

    /// Last cursor position (for detecting cursor movement)
    pub last_cursor_pos: usize,

    /// Time (in seconds since app start) when cursor was last moved
    /// Used to reset cursor blink animation after movement
    pub cursor_moved_time: f64,

    /// Last cursor position for blink reset tracking (separate from last_cursor_pos)
    /// This is tracked independently to avoid race conditions with auto_scroll_to_cursor
    pub last_cursor_pos_for_blink: usize,

    /// All cursors (including primary cursor at index 0)
    /// The first cursor is the "primary" cursor that maps to cursor_pos/selection_start/selection_end
    pub cursors: Vec<Cursor>,
}

impl Default for CursorState {
    fn default() -> Self {
        Self {
            cursor_pos: 0,
            last_cursor_pos: 0,
            cursor_moved_time: 0.0,
            last_cursor_pos_for_blink: 0,
            cursors: vec![Cursor::new(0)],
        }
    }
}

/// Main editor state — focus tracking and marker.
///
/// Most fields have been extracted into focused components:
/// - `SelectionState` — selection_start, selection_end, selections
/// - `EditHistoryState` — history, anchors
/// - `SyntaxCacheState` — tokens, lines, last_highlighted_version, last_lines_version, tree-sitter state
/// - `EditorDisplayState` — display_map, entity_pool, line_number_pool, invalidate_lines_from
/// - `CursorState` — cursor_pos, cursors, etc.
/// - `TextViewState` — rope, scroll, rendering state
#[derive(Component)]
pub struct CodeEditorState {
    /// Is editor focused
    pub is_focused: bool,
}

impl Default for CodeEditorState {
    fn default() -> Self {
        Self {
            is_focused: false,
        }
    }
}

/// Selection state component — tracks selection positions and the SelectionCollection.
#[derive(Component)]
pub struct SelectionState {
    /// Selection start (None = no selection) - primary cursor for backward compatibility
    pub selection_start: Option<usize>,
    /// Selection end - primary cursor for backward compatibility
    pub selection_end: Option<usize>,
    /// Selection collection for managing multiple selections with edit-awareness
    pub selections: SelectionCollection,
}

impl Default for SelectionState {
    fn default() -> Self {
        Self {
            selection_start: None,
            selection_end: None,
            selections: SelectionCollection::new(),
        }
    }
}

/// Edit history and anchor state component.
#[derive(Component)]
pub struct EditHistoryState {
    /// Edit history for undo/redo
    pub history: EditHistory,
    /// Anchor set for edit-resilient position tracking
    pub anchors: AnchorSet,
}

impl Default for EditHistoryState {
    fn default() -> Self {
        Self {
            history: EditHistory::default(),
            anchors: AnchorSet::new(),
        }
    }
}

/// Syntax highlighting cache state component.
#[derive(Component)]
pub struct SyntaxCacheState {
    /// Cached highlighted tokens
    pub tokens: Vec<HighlightedToken>,
    /// Cached processed lines for rendering (optimization)
    pub lines: Vec<Vec<LineSegment>>,
    /// Last content version when highlighting was run
    pub last_highlighted_version: u64,
    /// Last content version when line segments were built (PERFORMANCE)
    pub last_lines_version: u64,
    /// Last syntax tree version that was rendered (PERFORMANCE)
    #[cfg(feature = "tree-sitter")]
    pub last_rendered_tree_version: u64,
    /// Pending text edit for tree-sitter incremental parsing
    /// Format: (start_byte, old_end_byte, new_end_byte)
    #[cfg(feature = "tree-sitter")]
    pub pending_tree_sitter_edit: Option<(usize, usize, usize)>,
}

impl Default for SyntaxCacheState {
    fn default() -> Self {
        Self {
            tokens: Vec::new(),
            lines: Vec::new(),
            last_highlighted_version: u64::MAX, // Force initial highlighting
            last_lines_version: 0,
            #[cfg(feature = "tree-sitter")]
            last_rendered_tree_version: 0,
            #[cfg(feature = "tree-sitter")]
            pending_tree_sitter_edit: None,
        }
    }
}

/// Editor display state component — entity pools, display map, line invalidation.
#[derive(Component)]
pub struct EditorDisplayState {
    /// Display map for soft line wrapping
    pub display_map: DisplayMap,
    /// Pool of reusable text entities (PERFORMANCE)
    pub entity_pool: Vec<Entity>,
    /// Pool of reusable line number entities (PERFORMANCE)
    pub line_number_pool: Vec<Entity>,
    /// When line count changes, stores the line index from which all subsequent
    /// line entities should be invalidated.
    pub invalidate_lines_from: Option<usize>,
}

impl Default for EditorDisplayState {
    fn default() -> Self {
        Self {
            display_map: DisplayMap::default(),
            entity_pool: Vec::new(),
            line_number_pool: Vec::new(),
            invalidate_lines_from: None,
        }
    }
}

impl CodeEditorState {
    /// Create new editor state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get text content as string (delegates to the rope)
    pub fn text(&self, rope: &Rope) -> String {
        rope.to_string()
    }

    /// Get line count (delegates to the rope)
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

    /// Record a text edit for incremental parsing (sends TextEditEvent)
    /// No-op stub kept for backwards compatibility.
    pub fn record_edit(&self, _start_byte: usize, _old_end_byte: usize, _new_end_byte: usize) {
        // No-op: Event sending happens in plugin layer by detecting content_version changes
    }

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

        // Find start of word
        let mut start = pos;
        while start > 0 {
            let prev = rope.char(start - 1);
            if prev.is_alphanumeric() || prev == '_' {
                start -= 1;
            } else {
                break;
            }
        }

        // Find end of word
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
    pub fn find_next_occurrence(&self, rope: &Rope, text: &str, after_pos: usize) -> Option<(usize, usize)> {
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
    pub fn add_cursor_at_next_occurrence(&self, sel: &mut SelectionState, cursor: &mut CursorState, tv: &mut TextViewState) -> bool {
        // Get the text to search for
        let search_text = if let Some(primary) = cursor.cursors.first() {
            if primary.has_selection() {
                let (start, end) = (primary.selection_start(), primary.selection_end());
                tv.rope.slice(start..end).to_string()
            } else {
                // No selection - select word at cursor first
                if let Some((start, end)) = self.word_at_position(&tv.rope, primary.position) {
                    // Select the word at the primary cursor
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

        // Find the last cursor's selection end to search from
        let search_from = cursor
            .cursors
            .iter()
            .map(|c| c.selection_end())
            .max()
            .unwrap_or(0);

        // Find next occurrence
        if let Some((start, end)) = self.find_next_occurrence(&tv.rope, &search_text, search_from) {
            // Check if this position is already covered by an existing cursor
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
    pub fn add_cursor(&mut self, cursor: &mut CursorState, tv: &mut TextViewState, position: usize) {
        let position = position.min(tv.rope.len_chars());
        // Don't add duplicate cursor at same position
        if !cursor.cursors.iter().any(|c| c.position == position) {
            cursor.cursors.push(Cursor::new(position));
            self.sort_and_merge_cursors(cursor);
            tv.pending_update = true;
        }
    }

    /// Add a new cursor with selection
    pub fn add_cursor_with_selection(&mut self, cursor: &mut CursorState, tv: &mut TextViewState, position: usize, anchor: usize) {
        let position = position.min(tv.rope.len_chars());
        let anchor = anchor.min(tv.rope.len_chars());
        cursor.cursors.push(Cursor::with_selection(position, anchor));
        self.sort_and_merge_cursors(cursor);
        tv.pending_update = true;
    }

    /// Remove all cursors except the primary one
    pub fn clear_secondary_cursors(&mut self, cursor: &mut CursorState, tv: &mut TextViewState) {
        if !cursor.cursors.is_empty() {
            cursor.cursors.truncate(1);
        }
        self.sync_primary_cursor(cursor);
        tv.pending_update = true;
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

        // Sort by position
        cursor.cursors.sort_by_key(|c| c.position);

        // Merge overlapping selections
        let mut merged: Vec<Cursor> = Vec::with_capacity(cursor.cursors.len());
        for c in cursor.cursors.drain(..) {
            if let Some(last) = merged.last_mut() {
                let last_end = last.selection_end();
                let cursor_start = c.selection_start();

                // If selections overlap or are adjacent, merge them
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

        // Update primary cursor from the first cursor
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
    pub fn add_selection(&mut self, cursor: &mut CursorState, tv: &mut TextViewState, offset: usize) {
        let offset = offset.min(tv.rope.len_chars());
        self.selections.add_cursor(offset);
        self.sync_from_selections(cursor);
        tv.pending_update = true;
    }

    /// Add a new selection with a range
    pub fn add_selection_range(&mut self, cursor: &mut CursorState, tv: &mut TextViewState, head: usize, anchor: usize) {
        let head = head.min(tv.rope.len_chars());
        let anchor = anchor.min(tv.rope.len_chars());
        self.selections.add_selection_range(head, anchor);
        self.sync_from_selections(cursor);
        tv.pending_update = true;
    }

    /// Clear all secondary selections, keeping only the primary
    pub fn clear_secondary_selections_sel(&mut self, cursor: &mut CursorState, tv: &mut TextViewState) {
        self.selections.clear_secondary();
        self.sync_from_selections(cursor);
        tv.pending_update = true;
    }

    /// Move the primary selection to a new position
    pub fn set_primary_selection(&mut self, cursor: &mut CursorState, tv: &mut TextViewState, head: usize, extend: bool) {
        let head = head.min(tv.rope.len_chars());
        self.selections.move_primary(head, extend);
        self.sync_from_selections(cursor);
        tv.pending_update = true;
    }
}

impl EditHistoryState {
    /// Insert character at cursor position (with undo recording)
    pub fn insert_char(&mut self, sel: &mut SelectionState, syntax: &mut SyntaxCacheState, display: &mut EditorDisplayState, cursor: &mut CursorState, tv: &mut TextViewState, c: char) {
        self.insert_char_with_history(sel, syntax, display, cursor, tv, c, true);
    }

    /// Insert character at cursor position with optional history recording
    pub fn insert_char_with_history(&mut self, sel: &mut SelectionState, syntax: &mut SyntaxCacheState, display: &mut EditorDisplayState, cursor: &mut CursorState, tv: &mut TextViewState, c: char, record_history: bool) {
        let cursor_pos = cursor.cursor_pos.min(tv.rope.len_chars());
        let line_idx = tv.rope.char_to_line(cursor_pos);
        let cursor_before = cursor_pos;

        #[cfg(feature = "tree-sitter")]
        let start_byte = tv.rope.char_to_byte(cursor_pos);
        #[cfg(feature = "tree-sitter")]
        let char_byte_len = c.len_utf8();

        self.anchors.record_edit(TextEdit::insert(cursor_pos, 1));

        tv.rope.insert_char(cursor_pos, c);
        cursor.cursor_pos += 1;
        sel.sync_cursors_from_primary(cursor);
        tv.pending_update = true;
        tv.content_version += 1;

        #[cfg(feature = "tree-sitter")]
        {
            syntax.pending_tree_sitter_edit = Some((
                start_byte,
                start_byte,
                start_byte + char_byte_len,
            ));
        }

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

        let new_line_count = tv.rope.len_lines();
        tv.dirty_lines = Some(line_idx..(line_idx + 1).min(new_line_count));

        if c == '\n' {
            display.invalidate_lines_from = Some(line_idx);
        }

        tv.previous_line_count = new_line_count;
    }

    /// Delete character before cursor (with undo recording)
    pub fn delete_backward(&mut self, sel: &mut SelectionState, syntax: &mut SyntaxCacheState, display: &mut EditorDisplayState, cursor: &mut CursorState, tv: &mut TextViewState) {
        self.delete_backward_with_history(sel, syntax, display, cursor, tv, true);
    }

    /// Delete character before cursor with optional history recording
    pub fn delete_backward_with_history(&mut self, sel: &mut SelectionState, syntax: &mut SyntaxCacheState, display: &mut EditorDisplayState, cursor: &mut CursorState, tv: &mut TextViewState, record_history: bool) {
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
            sel.sync_cursors_from_primary(cursor);
            tv.pending_update = true;
            tv.content_version += 1;

            #[cfg(feature = "tree-sitter")]
            {
                syntax.pending_tree_sitter_edit = Some((char_idx, byte_idx_end, char_idx));
            }

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

            let new_line_count = tv.rope.len_lines();
            tv.dirty_lines = Some(line_idx..(line_idx + 1).min(new_line_count));

            if deleted_char == '\n' {
                display.invalidate_lines_from = Some(line_idx);
            }

            tv.previous_line_count = new_line_count;
        }
    }

    /// Delete character after cursor (with undo recording)
    pub fn delete_forward(&mut self, sel: &mut SelectionState, syntax: &mut SyntaxCacheState, display: &mut EditorDisplayState, cursor: &mut CursorState, tv: &mut TextViewState) {
        self.delete_forward_with_history(sel, syntax, display, cursor, tv, true);
    }

    /// Delete character after cursor with optional history recording
    pub fn delete_forward_with_history(&mut self, sel: &mut SelectionState, syntax: &mut SyntaxCacheState, display: &mut EditorDisplayState, cursor: &mut CursorState, tv: &mut TextViewState, record_history: bool) {
        if cursor.cursor_pos < tv.rope.len_chars() {
            let cursor_before = cursor.cursor_pos;
            let line_idx = tv.rope.char_to_line(cursor.cursor_pos);
            let deleted_char = tv.rope.char(cursor.cursor_pos);
            let char_idx = tv.rope.char_to_byte(cursor.cursor_pos);
            let byte_idx_end = tv.rope.char_to_byte(cursor.cursor_pos + 1);

            self.anchors
                .record_edit(TextEdit::delete(cursor.cursor_pos, cursor.cursor_pos + 1));

            tv.rope.remove(char_idx..byte_idx_end);
            sel.sync_cursors_from_primary(cursor);
            tv.pending_update = true;
            tv.content_version += 1;

            #[cfg(feature = "tree-sitter")]
            {
                syntax.pending_tree_sitter_edit = Some((char_idx, byte_idx_end, char_idx));
            }

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

            let new_line_count = tv.rope.len_lines();
            tv.dirty_lines = Some(line_idx..(line_idx + 1).min(new_line_count));

            if deleted_char == '\n' {
                display.invalidate_lines_from = Some(line_idx);
            }

            tv.previous_line_count = new_line_count;
        }
    }

    /// Insert text at a specific position (used for undo/redo)
    pub fn insert_text_at(&mut self, syntax: &mut SyntaxCacheState, display: &mut EditorDisplayState, tv: &mut TextViewState, pos: usize, text: &str) {
        let pos = pos.min(tv.rope.len_chars());
        let text_char_len = text.chars().count();
        let line_idx = tv.rope.char_to_line(pos);

        #[cfg(feature = "tree-sitter")]
        let start_byte = tv.rope.char_to_byte(pos);
        #[cfg(feature = "tree-sitter")]
        let text_byte_len = text.len();

        self.anchors
            .record_edit(TextEdit::insert(pos, text_char_len));

        tv.rope.insert(pos, text);
        tv.pending_update = true;
        tv.content_version += 1;
        tv.dirty_lines = None;

        if text.contains('\n') {
            display.invalidate_lines_from = Some(line_idx);
        }

        tv.previous_line_count = tv.rope.len_lines();

        #[cfg(feature = "tree-sitter")]
        {
            syntax.pending_tree_sitter_edit = Some((
                start_byte,
                start_byte,
                start_byte + text_byte_len,
            ));
        }
    }

    /// Remove text range (used for undo/redo)
    pub fn remove_range(&mut self, syntax: &mut SyntaxCacheState, display: &mut EditorDisplayState, tv: &mut TextViewState, start: usize, end: usize) {
        let start = start.min(tv.rope.len_chars());
        let end = end.min(tv.rope.len_chars());
        if start < end {
            let line_idx = tv.rope.char_to_line(start);
            let start_byte = tv.rope.char_to_byte(start);
            let end_byte = tv.rope.char_to_byte(end);

            let removed_text: String = tv.rope.slice(start..end).chars().collect();
            let has_newlines = removed_text.contains('\n');

            self.anchors.record_edit(TextEdit::delete(start, end));

            tv.rope.remove(start_byte..end_byte);
            tv.pending_update = true;
            tv.content_version += 1;
            tv.dirty_lines = None;

            if has_newlines {
                display.invalidate_lines_from = Some(line_idx);
            }

            tv.previous_line_count = tv.rope.len_lines();

            #[cfg(feature = "tree-sitter")]
            {
                syntax.pending_tree_sitter_edit = Some((start_byte, end_byte, start_byte));
            }
        }
    }

    /// Perform undo operation
    pub fn undo(&mut self, syntax: &mut SyntaxCacheState, display: &mut EditorDisplayState, cursor: &mut CursorState, tv: &mut TextViewState) -> bool {
        if let Some(transaction) = self.history.pop_undo() {
            for op in transaction.operations.iter().rev() {
                if !op.inserted_text.is_empty() {
                    let end_pos = op.position + op.inserted_text.chars().count();
                    self.remove_range(syntax, display, tv, op.position, end_pos);
                }
                if !op.removed_text.is_empty() {
                    self.insert_text_at(syntax, display, tv, op.position, &op.removed_text);
                }
            }

            if let Some(first_op) = transaction.operations.first() {
                cursor.cursor_pos = first_op.cursor_before;
            }

            self.history.push_redo(transaction);
            true
        } else {
            false
        }
    }

    /// Perform redo operation
    pub fn redo(&mut self, syntax: &mut SyntaxCacheState, display: &mut EditorDisplayState, cursor: &mut CursorState, tv: &mut TextViewState) -> bool {
        if let Some(transaction) = self.history.pop_redo() {
            for op in transaction.operations.iter() {
                if !op.removed_text.is_empty() {
                    let end_pos = op.position + op.removed_text.chars().count();
                    self.remove_range(syntax, display, tv, op.position, end_pos);
                }
                if !op.inserted_text.is_empty() {
                    self.insert_text_at(syntax, display, tv, op.position, &op.inserted_text);
                }
            }

            if let Some(last_op) = transaction.operations.last() {
                cursor.cursor_pos = last_op.cursor_after;
            }

            self.history.push_undo(transaction);
            true
        } else {
            false
        }
    }

    /// Set text content
    pub fn set_text(&mut self, sel: &mut SelectionState, syntax: &mut SyntaxCacheState, cursor: &mut CursorState, tv: &mut TextViewState, text: &str) {
        #[cfg(feature = "tree-sitter")]
        let old_byte_len = tv.rope.len_bytes();
        #[cfg(feature = "tree-sitter")]
        let new_byte_len = text.len();

        tv.rope = Rope::from_str(text);
        cursor.cursor_pos = cursor.cursor_pos.min(tv.rope.len_chars());
        tv.pending_update = true;
        tv.content_version += 1;
        tv.dirty_lines = None;
        tv.previous_line_count = tv.rope.len_lines();
        self.anchors.clear();
        sel.selections = SelectionCollection::with_cursor(cursor.cursor_pos);
        tv.line_width_tracker.rebuild(&tv.rope);
        tv.max_content_width_version = 0;

        #[cfg(feature = "tree-sitter")]
        {
            syntax.pending_tree_sitter_edit = Some((0, old_byte_len, new_byte_len));
        }
    }

    // ========== Anchor methods ==========

    /// Create an anchor at the given position with left bias
    pub fn create_anchor(&mut self, rope: &Rope, offset: usize, bias: AnchorBias) -> Anchor {
        let offset = offset.min(rope.len_chars());
        self.anchors.anchor_at(offset, bias)
    }

    /// Create an anchor at the given position with left bias (cursor-like behavior)
    pub fn anchor_at(&mut self, rope: &Rope, offset: usize) -> Anchor {
        self.create_anchor(rope, offset, AnchorBias::Left)
    }

    /// Resolve an anchor's current position (applies pending edits)
    pub fn resolve_anchor(&self, rope: &Rope, anchor: &Anchor) -> usize {
        self.anchors.resolve(anchor).min(rope.len_chars())
    }

    /// Apply pending anchor edits
    pub fn apply_anchor_edits(&mut self) {
        self.anchors.apply_pending_edits();
    }

    /// Remove an anchor by its ID
    pub fn remove_anchor(&mut self, id: u64) -> Option<Anchor> {
        self.anchors.remove(id)
    }
}

/// Component markers for editor entities

#[derive(Component)]
pub struct EditorText;

#[derive(Component)]
pub struct HighlightedTextToken {
    pub index: usize,
}

#[derive(Component)]
pub struct EditorCursor {
    /// Index of this cursor in the cursors array (0 = primary cursor)
    pub cursor_index: usize,
}

#[derive(Component)]
pub struct LineNumbers;

#[derive(Component)]
pub struct Separator;

#[derive(Component)]
pub struct SelectionHighlight {
    pub line_index: usize,
    /// Index of the cursor this selection belongs to (0 = primary cursor)
    pub cursor_index: usize,
}

/// Component marker for bracket match highlight entities (bounding box style)
#[derive(Component)]
pub struct BracketMatchHighlight {
    /// Which bracket this belongs to (0 = cursor bracket, 1 = matching bracket)
    pub bracket_index: usize,
    /// Which border edge (0=top, 1=bottom, 2=left, 3=right)
    pub edge: usize,
}

/// Component marker for current line border (top or bottom line)
#[derive(Component)]
pub struct CursorLineBorder {
    /// The cursor index this border belongs to (for multi-cursor support)
    pub cursor_index: usize,
    /// Whether this is the top (true) or bottom (false) border
    pub is_top: bool,
}

/// Component marker for current word highlight (under cursor)
#[derive(Component)]
pub struct CursorWordHighlight {
    /// The cursor index this highlight belongs to (for multi-cursor support)
    pub cursor_index: usize,
}

/// Component marker for indent guide entities
#[derive(Component)]
pub struct IndentGuide {
    /// The indentation level (0 = first indent, 1 = second indent, etc.)
    pub level: usize,
    /// The line index this guide is on
    pub line_index: usize,
}

/// Component marker for the minimap background
#[derive(Component)]
pub struct MinimapBackground;

/// Component marker for the minimap viewport slider (appears on hover)
#[derive(Component)]
pub struct MinimapSlider;

/// Component marker for the minimap viewport highlight (subtle, always visible)
#[derive(Component)]
pub struct MinimapViewportHighlight;

/// Component marker for the minimap scrollbar
#[derive(Component)]
pub struct MinimapScrollbar;

/// Component marker for minimap line entities
#[derive(Component)]
pub struct MinimapLine {
    /// The line index this represents
    pub line_index: usize,
}

/// Component marker for minimap search match highlights
#[derive(Component)]
pub struct MinimapFindHighlight {
    /// The line index this highlight represents
    pub line_index: usize,
}

/// Component marker for GPU minimap mesh entity
#[derive(Component)]
pub struct GpuMinimapMesh {
    /// The content version when this mesh was built
    pub built_at_version: u64,
    /// The scroll offset when this mesh was built
    pub built_at_scroll: f32,
    /// The viewport width when this mesh was built
    pub built_at_width: u32,
    /// The viewport height when this mesh was built
    pub built_at_height: u32,
    /// The viewport offset_x when this mesh was built
    pub built_at_offset_x: f32,
}

/// Component marker for the minimap camera
#[derive(Component)]
pub struct MinimapCamera;

/// Resource to track minimap hover state
#[derive(Resource, Default)]
pub struct MinimapHoverState {
    /// Whether the mouse is currently hovering over the minimap
    pub is_hovered: bool,
}

/// Resource to track minimap drag state for click-to-scroll and drag-to-scroll
#[derive(Resource, Default)]
pub struct MinimapDragState {
    /// Whether we're currently dragging the minimap slider
    pub is_dragging: bool,
    /// Whether we're dragging the viewport highlight (vs clicking elsewhere on minimap)
    pub is_dragging_highlight: bool,
    /// Initial mouse Y position when drag started (for highlight dragging)
    pub drag_start_y: f32,
    /// Initial scroll offset when drag started (for highlight dragging)
    pub drag_start_scroll: f32,
}

/// Resource to track key repeat state for editor actions
#[derive(Resource, Default)]
pub struct KeyRepeatState {
    /// The action currently being repeated (if any)
    pub current_action: Option<crate::input::EditorAction>,
    /// When the action key was first pressed
    pub press_start: Option<Instant>,
    /// When the last repeat occurred
    pub last_repeat: Option<Instant>,
}

/// Represents a matched bracket pair
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BracketMatch {
    /// Position of the bracket at/near cursor
    pub cursor_bracket_pos: usize,
    /// Position of the matching bracket
    pub matching_bracket_pos: usize,
}

/// Resource to track the current bracket match state
#[derive(Resource, Default, Clone, Debug)]
pub struct BracketMatchState {
    /// Current bracket match (if any)
    pub current_match: Option<BracketMatch>,
}

/// Component marker for find/search highlight entities
#[derive(Component)]
pub struct FindHighlight {
    /// Index of this match in the matches list
    pub match_index: usize,
}

/// Event emitted when save is requested (Ctrl+S)
/// The host application should handle this event to save the buffer contents.
#[derive(bevy::prelude::Message, Clone, Debug)]
pub struct SaveRequested {
    /// The current buffer content
    pub content: String,
}

/// Event emitted when open is requested (Ctrl+O)
/// The host application should handle this event to show a file picker.
#[derive(bevy::prelude::Message, Clone, Debug)]
pub struct OpenRequested;

// ========== External Scroll Control ==========

/// Resource for external UI (egui) to control and read editor scroll state
///
/// This provides bidirectional communication:
/// - External UI reads current scroll position and content dimensions
/// - External UI sends scroll requests via pending_* fields
/// - bevy_code_editor's systems apply these requests and update current state
#[derive(Resource, Default)]
pub struct EditorScrollControl {
    /// Current vertical scroll offset (negative when scrolled down)
    /// Updated by bevy_code_editor every frame
    pub scroll_offset: f32,

    /// Current horizontal scroll offset
    /// Updated by bevy_code_editor every frame
    pub horizontal_scroll_offset: f32,

    /// Total content height in pixels
    pub content_height: f32,

    /// Total content width in pixels (longest line)
    pub content_width: f32,

    /// Visible viewport height in pixels
    pub viewport_height: f32,

    /// Visible viewport width in pixels (excluding gutter)
    pub viewport_width: f32,

    /// Line height for line-based scrolling
    pub line_height: f32,

    /// Pending vertical scroll request (absolute offset to scroll to)
    /// Set by external UI, cleared by bevy_code_editor when applied
    pub pending_scroll_to: Option<f32>,

    /// Pending vertical scroll delta (relative scroll amount)
    /// Set by external UI, cleared by bevy_code_editor when applied
    pub pending_scroll_delta: Option<f32>,

    /// Pending horizontal scroll request (absolute offset)
    pub pending_horizontal_scroll_to: Option<f32>,

    /// Pending horizontal scroll delta (relative amount)
    pub pending_horizontal_scroll_delta: Option<f32>,
}

