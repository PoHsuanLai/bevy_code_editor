//! Selection and cursor types

use std::sync::atomic::{AtomicU64, Ordering};

use super::anchor::{Anchor, AnchorSet, TextEdit};

// ========== Selection Collection ==========

/// A selection represents a cursor position with an optional anchor for text selection.
/// Uses anchors for edit-resilience, meaning positions automatically adjust when text is edited.
///
/// The selection is defined by:
/// - `head`: The cursor position (where the cursor is displayed, with Left bias)
/// - `anchor`: The selection anchor (where the selection started, with Right bias)
///
/// When `head == anchor`, there's no selection (just a cursor).
/// The head and anchor can be in any order - head can be before or after anchor.
#[derive(Clone, Debug)]
pub struct Selection {
    /// The cursor position (where the cursor blinks)
    /// Uses Left bias so it stays before inserted text
    pub head: Anchor,
    /// The selection anchor (where selection started)
    /// Uses Right bias so selection expands to include inserted text at the boundary
    pub anchor: Anchor,
    /// Unique ID for this selection (for tracking across operations)
    id: u64,
}

/// Global counter for generating unique selection IDs
static SELECTION_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

impl Selection {
    /// Create a new selection with just a cursor (no selection)
    pub fn cursor(offset: usize) -> Self {
        Self {
            head: Anchor::at(offset),
            anchor: Anchor::at(offset),
            id: SELECTION_ID_COUNTER.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// Create a new selection with a range
    /// `head` is where the cursor is, `anchor` is where the selection started
    pub fn new(head: usize, anchor: usize) -> Self {
        Self {
            head: Anchor::at(head),
            anchor: Anchor::at_right(anchor),
            id: SELECTION_ID_COUNTER.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// Create a selection from anchor objects
    pub fn from_anchors(head: Anchor, anchor: Anchor) -> Self {
        Self {
            head,
            anchor,
            id: SELECTION_ID_COUNTER.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// Get the unique ID of this selection
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Get the head (cursor) position
    pub fn head_offset(&self) -> usize {
        self.head.offset
    }

    /// Get the anchor position
    pub fn anchor_offset(&self) -> usize {
        self.anchor.offset
    }

    /// Get the start position (minimum of head and anchor)
    pub fn start(&self) -> usize {
        self.head.offset.min(self.anchor.offset)
    }

    /// Get the end position (maximum of head and anchor)
    pub fn end(&self) -> usize {
        self.head.offset.max(self.anchor.offset)
    }

    /// Get the range as (start, end) tuple, always ordered
    pub fn range(&self) -> (usize, usize) {
        (self.start(), self.end())
    }

    /// Check if this is just a cursor (no selection)
    pub fn is_cursor(&self) -> bool {
        self.head.offset == self.anchor.offset
    }

    /// Check if there is an actual selection (head != anchor)
    pub fn has_selection(&self) -> bool {
        self.head.offset != self.anchor.offset
    }

    /// Check if the selection is "reversed" (anchor is after head)
    pub fn is_reversed(&self) -> bool {
        self.anchor.offset > self.head.offset
    }

    /// Check if a position is within the selected range
    pub fn contains(&self, offset: usize) -> bool {
        let (start, end) = self.range();
        offset >= start && offset < end
    }

    /// Check if this selection overlaps with another
    pub fn overlaps(&self, other: &Selection) -> bool {
        let (s1, e1) = self.range();
        let (s2, e2) = other.range();
        s1 < e2 && s2 < e1
    }

    /// Check if this selection is adjacent to another (touching but not overlapping)
    pub fn is_adjacent(&self, other: &Selection) -> bool {
        let (_, e1) = self.range();
        let (s2, _) = other.range();
        e1 == s2
    }

    /// Check if this selection can be merged with another (overlapping or adjacent)
    pub fn can_merge(&self, other: &Selection) -> bool {
        self.overlaps(other) || self.is_adjacent(other) || other.is_adjacent(self)
    }

    /// Merge this selection with another, returning the merged selection
    /// The head position comes from `self` (the "primary" selection in the merge)
    pub fn merge(&self, other: &Selection) -> Selection {
        let new_start = self.start().min(other.start());
        let new_end = self.end().max(other.end());

        // Preserve the head direction from self
        if self.is_reversed() {
            Selection::new(new_start, new_end)
        } else {
            Selection::new(new_end, new_start)
        }
    }

    /// Adjust this selection based on a text edit
    pub fn adjust(&mut self, edit: &TextEdit) {
        self.head.offset = AnchorSet::adjust_offset(self.head.offset, self.head.bias, edit);
        self.anchor.offset = AnchorSet::adjust_offset(self.anchor.offset, self.anchor.bias, edit);
    }

    /// Move the head to a new position, optionally extending the selection
    pub fn move_head(&mut self, offset: usize, extend: bool) {
        self.head.offset = offset;
        if !extend {
            self.anchor.offset = offset;
        }
    }

    /// Collapse the selection to just a cursor at the head position
    pub fn collapse_to_head(&mut self) {
        self.anchor.offset = self.head.offset;
    }

    /// Collapse the selection to just a cursor at the start position
    pub fn collapse_to_start(&mut self) {
        let start = self.start();
        self.head.offset = start;
        self.anchor.offset = start;
    }

    /// Collapse the selection to just a cursor at the end position
    pub fn collapse_to_end(&mut self) {
        let end = self.end();
        self.head.offset = end;
        self.anchor.offset = end;
    }

    /// Get the length of the selection (0 if just a cursor)
    pub fn len(&self) -> usize {
        self.end() - self.start()
    }
}

impl PartialEq for Selection {
    fn eq(&self, other: &Self) -> bool {
        self.head.offset == other.head.offset && self.anchor.offset == other.anchor.offset
    }
}

impl Eq for Selection {}

impl PartialOrd for Selection {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Selection {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Sort by start position, then by end position
        self.start()
            .cmp(&other.start())
            .then_with(|| self.end().cmp(&other.end()))
    }
}

/// A collection of non-overlapping selections, maintained in sorted order.
///
/// This is the primary interface for managing multiple selections in the editor.
/// It automatically:
/// - Keeps selections sorted by position
/// - Merges overlapping and adjacent selections
/// - Adjusts all selections when text is edited
///
/// The first selection (index 0) is the "primary" selection that determines
/// the main cursor position for scrolling and other operations.
#[derive(Clone, Debug)]
pub struct SelectionCollection {
    /// The selections, maintained in sorted order by start position
    /// Index 0 is the "primary" selection
    selections: Vec<Selection>,
    /// Pending edits to apply to all selections
    pending_edits: Vec<TextEdit>,
    /// Version counter for tracking changes
    version: u64,
}

impl Default for SelectionCollection {
    fn default() -> Self {
        Self::new()
    }
}

impl SelectionCollection {
    /// Create a new collection with a single cursor at position 0
    pub fn new() -> Self {
        Self {
            selections: vec![Selection::cursor(0)],
            pending_edits: Vec::new(),
            version: 0,
        }
    }

    /// Create a collection with a single cursor at the given position
    pub fn with_cursor(offset: usize) -> Self {
        Self {
            selections: vec![Selection::cursor(offset)],
            pending_edits: Vec::new(),
            version: 0,
        }
    }

    /// Create a collection with a single selection
    pub fn with_selection(head: usize, anchor: usize) -> Self {
        Self {
            selections: vec![Selection::new(head, anchor)],
            pending_edits: Vec::new(),
            version: 0,
        }
    }

    /// Get the primary selection (first selection)
    pub fn primary(&self) -> &Selection {
        // There's always at least one selection
        &self.selections[0]
    }

    /// Get a mutable reference to the primary selection
    pub fn primary_mut(&mut self) -> &mut Selection {
        &mut self.selections[0]
    }

    /// Get the primary cursor position (head of primary selection)
    pub fn cursor_pos(&self) -> usize {
        self.primary().head_offset()
    }

    /// Get the number of selections
    pub fn len(&self) -> usize {
        self.selections.len()
    }

    /// Check if there's only a single cursor (no multi-selection, no text selected)
    pub fn is_single_cursor(&self) -> bool {
        self.selections.len() == 1 && self.selections[0].is_cursor()
    }

    /// Check if any selection has text selected
    pub fn has_selection(&self) -> bool {
        self.selections.iter().any(|s| s.has_selection())
    }

    /// Check if there are multiple selections
    pub fn is_multiple(&self) -> bool {
        self.selections.len() > 1
    }

    /// Iterate over all selections
    pub fn iter(&self) -> impl Iterator<Item = &Selection> {
        self.selections.iter()
    }

    /// Iterate over all selections mutably
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Selection> {
        self.selections.iter_mut()
    }

    /// Get a selection by index
    pub fn get(&self, index: usize) -> Option<&Selection> {
        self.selections.get(index)
    }

    /// Get a mutable selection by index
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Selection> {
        self.selections.get_mut(index)
    }

    /// Add a new selection (cursor only) at the given position
    /// Returns the index of the new selection after sorting/merging
    pub fn add_cursor(&mut self, offset: usize) -> usize {
        self.add_selection(Selection::cursor(offset))
    }

    /// Add a new selection with a range
    /// Returns the index of the new selection after sorting/merging
    pub fn add_selection_range(&mut self, head: usize, anchor: usize) -> usize {
        self.add_selection(Selection::new(head, anchor))
    }

    /// Add a selection to the collection
    /// Automatically sorts and merges overlapping selections
    /// Returns the index of the added (or merged) selection
    pub fn add_selection(&mut self, selection: Selection) -> usize {
        self.selections.push(selection);
        self.sort_and_merge();
        self.version += 1;

        // Find the index of the selection we just added (it might have been merged)
        // For now, return the last index which is where we added it
        self.selections.len().saturating_sub(1)
    }

    /// Remove all selections except the primary
    pub fn clear_secondary(&mut self) {
        if self.selections.len() > 1 {
            self.selections.truncate(1);
            self.version += 1;
        }
    }

    /// Replace all selections with a single cursor
    pub fn set_cursor(&mut self, offset: usize) {
        self.selections.clear();
        self.selections.push(Selection::cursor(offset));
        self.version += 1;
    }

    /// Replace all selections with a single selection
    pub fn set_selection(&mut self, head: usize, anchor: usize) {
        self.selections.clear();
        self.selections.push(Selection::new(head, anchor));
        self.version += 1;
    }

    /// Move the primary selection's head, optionally extending
    pub fn move_primary(&mut self, offset: usize, extend: bool) {
        self.selections[0].move_head(offset, extend);
        self.version += 1;
    }

    /// Move all selection heads by applying a function
    pub fn move_all<F>(&mut self, mut f: F, extend: bool)
    where
        F: FnMut(usize) -> usize,
    {
        for selection in &mut self.selections {
            let new_pos = f(selection.head_offset());
            selection.move_head(new_pos, extend);
        }
        if !extend {
            // If not extending, selections might now be at the same position
            // and should be deduplicated
            self.sort_and_merge();
        }
        self.version += 1;
    }

    /// Collapse all selections to cursors at their head positions
    pub fn collapse_all_to_head(&mut self) {
        for selection in &mut self.selections {
            selection.collapse_to_head();
        }
        self.sort_and_merge();
        self.version += 1;
    }

    /// Collapse all selections to cursors at their start positions
    pub fn collapse_all_to_start(&mut self) {
        for selection in &mut self.selections {
            selection.collapse_to_start();
        }
        self.sort_and_merge();
        self.version += 1;
    }

    /// Collapse all selections to cursors at their end positions
    pub fn collapse_all_to_end(&mut self) {
        for selection in &mut self.selections {
            selection.collapse_to_end();
        }
        self.sort_and_merge();
        self.version += 1;
    }

    /// Record a text edit to adjust all selections
    pub fn record_edit(&mut self, edit: TextEdit) {
        self.pending_edits.push(edit);
        self.version += 1;
    }

    /// Apply all pending edits to selections
    pub fn apply_pending_edits(&mut self) {
        if self.pending_edits.is_empty() {
            return;
        }

        for selection in &mut self.selections {
            for edit in &self.pending_edits {
                selection.adjust(edit);
            }
        }

        self.pending_edits.clear();

        // Re-sort and merge after adjustments (edits might cause overlaps)
        self.sort_and_merge();
    }

    /// Sort selections by position and merge overlapping/adjacent ones
    fn sort_and_merge(&mut self) {
        if self.selections.len() <= 1 {
            return;
        }

        // Sort by start position
        self.selections.sort();

        // Merge overlapping and adjacent selections
        let mut merged: Vec<Selection> = Vec::with_capacity(self.selections.len());

        for selection in self.selections.drain(..) {
            if let Some(last) = merged.last_mut() {
                if last.can_merge(&selection) {
                    // Merge into the existing selection
                    *last = last.merge(&selection);
                } else {
                    merged.push(selection);
                }
            } else {
                merged.push(selection);
            }
        }

        self.selections = merged;

        // Ensure we always have at least one selection
        if self.selections.is_empty() {
            self.selections.push(Selection::cursor(0));
        }
    }

    /// Get the ranges of all selections as (start, end) tuples
    pub fn ranges(&self) -> Vec<(usize, usize)> {
        self.selections.iter().map(|s| s.range()).collect()
    }

    /// Get the version (incremented on changes)
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Check if any selection contains the given offset
    pub fn any_contains(&self, offset: usize) -> bool {
        self.selections.iter().any(|s| s.contains(offset))
    }

    /// Find the selection containing the given offset (if any)
    pub fn selection_at(&self, offset: usize) -> Option<&Selection> {
        self.selections.iter().find(|s| s.contains(offset))
    }

    /// Convert to a Vec of (head, anchor) tuples for compatibility
    pub fn to_head_anchor_pairs(&self) -> Vec<(usize, Option<usize>)> {
        self.selections
            .iter()
            .map(|s| {
                if s.is_cursor() {
                    (s.head_offset(), None)
                } else {
                    (s.head_offset(), Some(s.anchor_offset()))
                }
            })
            .collect()
    }

    /// Create from the legacy Cursor format
    pub fn from_cursors(cursors: &[Cursor]) -> Self {
        let selections: Vec<Selection> = cursors
            .iter()
            .map(|c| {
                if let Some(anchor) = c.anchor {
                    Selection::new(c.position, anchor)
                } else {
                    Selection::cursor(c.position)
                }
            })
            .collect();

        let mut collection = Self {
            selections,
            pending_edits: Vec::new(),
            version: 0,
        };
        collection.sort_and_merge();
        collection
    }

    /// Convert to the legacy Cursor format
    pub fn to_cursors(&self) -> Vec<Cursor> {
        self.selections
            .iter()
            .map(|s| {
                if s.is_cursor() {
                    Cursor::new(s.head_offset())
                } else {
                    Cursor::with_selection(s.head_offset(), s.anchor_offset())
                }
            })
            .collect()
    }
}

/// Represents a single cursor with optional selection
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cursor {
    /// Cursor position (character index)
    pub position: usize,
    /// Selection anchor (where selection started, if any)
    /// When there's a selection, the selected range is between anchor and position
    pub anchor: Option<usize>,
}

impl Cursor {
    /// Create a new cursor at the given position with no selection
    pub fn new(position: usize) -> Self {
        Self {
            position,
            anchor: None,
        }
    }

    /// Create a new cursor with a selection
    pub fn with_selection(position: usize, anchor: usize) -> Self {
        Self {
            position,
            anchor: Some(anchor),
        }
    }

    /// Get the selection range (if any), ordered from start to end
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        self.anchor.map(|anchor| {
            if anchor <= self.position {
                (anchor, self.position)
            } else {
                (self.position, anchor)
            }
        })
    }

    /// Check if this cursor has a selection
    pub fn has_selection(&self) -> bool {
        self.anchor.is_some() && self.anchor != Some(self.position)
    }

    /// Clear the selection
    pub fn clear_selection(&mut self) {
        self.anchor = None;
    }

    /// Start a selection at the current position if none exists
    pub fn start_selection(&mut self) {
        if self.anchor.is_none() {
            self.anchor = Some(self.position);
        }
    }

    /// Get the start of the selection (or cursor position if no selection)
    pub fn selection_start(&self) -> usize {
        self.anchor
            .map(|a| a.min(self.position))
            .unwrap_or(self.position)
    }

    /// Get the end of the selection (or cursor position if no selection)
    pub fn selection_end(&self) -> usize {
        self.anchor
            .map(|a| a.max(self.position))
            .unwrap_or(self.position)
    }
}
