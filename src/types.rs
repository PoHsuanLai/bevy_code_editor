//! Core types for the code editor

use bevy::prelude::*;
use ropey::Rope;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use crate::line_width::LineWidthTracker;

#[cfg(feature = "lsp")]
use lsp_types::Url;

// ========== Anchor-based Position Tracking ==========

/// Global counter for generating unique anchor IDs
static ANCHOR_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Bias determines how an anchor behaves when text is inserted exactly at its position
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum AnchorBias {
    /// Anchor stays before the inserted text (cursor-like behavior)
    #[default]
    Left,
    /// Anchor moves after the inserted text (selection-end-like behavior)
    Right,
}

/// An anchor is an edit-resilient position in the text buffer.
///
/// Unlike raw character offsets, anchors automatically adjust when text
/// is inserted or deleted around them. This makes them ideal for:
/// - Cursor positions that should stay at the "same place" after edits
/// - Selection boundaries
/// - Bookmarks
/// - Diagnostic positions from LSP
/// - Any position that needs to survive edits
///
/// # Example
/// ```ignore
/// // Create an anchor at position 10
/// let anchor = Anchor::new(10, AnchorBias::Left);
///
/// // If text is inserted at position 5, the anchor's resolved position becomes 15
/// // If text is inserted at position 10, the anchor stays at 10 (Left bias)
/// // If text is inserted at position 15, the anchor stays at 10
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Anchor {
    /// Unique identifier for this anchor (used for efficient lookup)
    pub id: u64,
    /// The character offset when this anchor was created or last resolved
    pub offset: usize,
    /// Determines behavior when text is inserted exactly at this position
    pub bias: AnchorBias,
    /// Version of the buffer when this anchor was last updated
    /// Used to detect if the anchor needs re-resolution
    pub version: u64,
}

impl Anchor {
    /// Create a new anchor at the given offset with the specified bias
    pub fn new(offset: usize, bias: AnchorBias) -> Self {
        Self {
            id: ANCHOR_ID_COUNTER.fetch_add(1, Ordering::Relaxed),
            offset,
            bias,
            version: 0,
        }
    }

    /// Create a new anchor at the given offset with left bias (default cursor behavior)
    pub fn at(offset: usize) -> Self {
        Self::new(offset, AnchorBias::Left)
    }

    /// Create a new anchor with right bias (for selection ends)
    pub fn at_right(offset: usize) -> Self {
        Self::new(offset, AnchorBias::Right)
    }

    /// Create an anchor at the start of the document
    pub fn start() -> Self {
        Self::new(0, AnchorBias::Left)
    }

    /// Create an anchor at the end of the document (will resolve to actual end)
    pub fn end() -> Self {
        Self::new(usize::MAX, AnchorBias::Right)
    }

    /// Get the current offset (may be stale if edits have occurred)
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Check if this anchor is at the start of the document
    pub fn is_at_start(&self) -> bool {
        self.offset == 0
    }
}

impl Default for Anchor {
    fn default() -> Self {
        Self::at(0)
    }
}

impl PartialOrd for Anchor {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Anchor {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.offset.cmp(&other.offset)
            .then_with(|| self.bias.cmp(&other.bias))
    }
}

impl PartialOrd for AnchorBias {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AnchorBias {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Left bias comes before Right bias at the same position
        match (self, other) {
            (AnchorBias::Left, AnchorBias::Right) => std::cmp::Ordering::Less,
            (AnchorBias::Right, AnchorBias::Left) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        }
    }
}

/// Represents a text edit operation for anchor adjustment
#[derive(Clone, Debug)]
pub struct TextEdit {
    /// Start position of the edit (character offset)
    pub start: usize,
    /// End position before the edit (character offset) - for deletions, this is > start
    pub old_end: usize,
    /// End position after the edit (character offset) - for insertions, this is > start
    pub new_end: usize,
}

impl TextEdit {
    /// Create an edit representing an insertion at the given position
    pub fn insert(position: usize, length: usize) -> Self {
        Self {
            start: position,
            old_end: position,
            new_end: position + length,
        }
    }

    /// Create an edit representing a deletion at the given range
    pub fn delete(start: usize, end: usize) -> Self {
        Self {
            start,
            old_end: end,
            new_end: start,
        }
    }

    /// Create an edit representing a replacement
    pub fn replace(start: usize, old_end: usize, new_length: usize) -> Self {
        Self {
            start,
            old_end,
            new_end: start + new_length,
        }
    }

    /// Get the change in length caused by this edit
    pub fn delta(&self) -> isize {
        self.new_end as isize - self.old_end as isize
    }

    /// Check if this edit is an insertion (no text removed)
    pub fn is_insertion(&self) -> bool {
        self.start == self.old_end && self.new_end > self.start
    }

    /// Check if this edit is a deletion (no text added)
    pub fn is_deletion(&self) -> bool {
        self.old_end > self.start && self.new_end == self.start
    }
}

/// A collection of anchors that can be efficiently updated when edits occur.
///
/// This is the main interface for managing edit-resilient positions. It tracks
/// all anchors and updates them in batch when text edits happen.
#[derive(Clone, Debug, Default)]
pub struct AnchorSet {
    /// All anchors, sorted by offset for efficient range queries
    anchors: Vec<Anchor>,
    /// Pending edits that haven't been applied yet
    pending_edits: Vec<TextEdit>,
    /// Current buffer version (incremented on each edit)
    version: u64,
}

impl AnchorSet {
    /// Create a new empty anchor set
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an anchor to the set and return its ID
    pub fn insert(&mut self, mut anchor: Anchor) -> u64 {
        anchor.version = self.version;
        let id = anchor.id;

        // Insert in sorted order by offset
        let pos = self.anchors
            .iter()
            .position(|a| a.offset > anchor.offset)
            .unwrap_or(self.anchors.len());
        self.anchors.insert(pos, anchor);

        id
    }

    /// Create and insert an anchor at the given offset
    pub fn anchor_at(&mut self, offset: usize, bias: AnchorBias) -> Anchor {
        let anchor = Anchor::new(offset, bias);
        self.insert(anchor);
        anchor
    }

    /// Remove an anchor by its ID
    pub fn remove(&mut self, id: u64) -> Option<Anchor> {
        if let Some(pos) = self.anchors.iter().position(|a| a.id == id) {
            Some(self.anchors.remove(pos))
        } else {
            None
        }
    }

    /// Get an anchor by its ID
    pub fn get(&self, id: u64) -> Option<&Anchor> {
        self.anchors.iter().find(|a| a.id == id)
    }

    /// Get a mutable reference to an anchor by its ID
    pub fn get_mut(&mut self, id: u64) -> Option<&mut Anchor> {
        self.anchors.iter_mut().find(|a| a.id == id)
    }

    /// Resolve an anchor's position, applying any pending edits
    pub fn resolve(&self, anchor: &Anchor) -> usize {
        let mut offset = anchor.offset;

        // Apply pending edits that occurred after this anchor was last updated
        for edit in &self.pending_edits {
            offset = Self::adjust_offset(offset, anchor.bias, edit);
        }

        offset
    }

    /// Record a text edit to adjust all anchors
    pub fn record_edit(&mut self, edit: TextEdit) {
        self.pending_edits.push(edit);
        self.version += 1;
    }

    /// Apply all pending edits to anchors
    pub fn apply_pending_edits(&mut self) {
        if self.pending_edits.is_empty() {
            return;
        }

        for anchor in &mut self.anchors {
            for edit in &self.pending_edits {
                anchor.offset = Self::adjust_offset(anchor.offset, anchor.bias, edit);
            }
            anchor.version = self.version;
        }

        self.pending_edits.clear();

        // Re-sort anchors after adjustment
        self.anchors.sort_by_key(|a| (a.offset, a.bias));
    }

    /// Adjust a single offset based on an edit
    fn adjust_offset(offset: usize, bias: AnchorBias, edit: &TextEdit) -> usize {
        if offset < edit.start {
            // Anchor is before the edit, no change needed
            offset
        } else if offset > edit.old_end {
            // Anchor is after the edit, shift by the delta
            let delta = edit.delta();
            if delta < 0 {
                offset.saturating_sub((-delta) as usize)
            } else {
                offset + delta as usize
            }
        } else if offset == edit.start && edit.is_insertion() {
            // Anchor is exactly at insertion point
            match bias {
                AnchorBias::Left => offset, // Stay before inserted text
                AnchorBias::Right => edit.new_end, // Move after inserted text
            }
        } else {
            // Anchor is within the deleted range
            // Move to the start of the edit (where deleted text was replaced)
            edit.start
        }
    }

    /// Clear all anchors
    pub fn clear(&mut self) {
        self.anchors.clear();
        self.pending_edits.clear();
    }

    /// Get the number of anchors
    pub fn len(&self) -> usize {
        self.anchors.len()
    }

    /// Check if the set is empty
    pub fn is_empty(&self) -> bool {
        self.anchors.is_empty()
    }

    /// Iterate over all anchors
    pub fn iter(&self) -> impl Iterator<Item = &Anchor> {
        self.anchors.iter()
    }

    /// Get anchors in a range of offsets
    pub fn anchors_in_range(&self, start: usize, end: usize) -> impl Iterator<Item = &Anchor> {
        self.anchors.iter().filter(move |a| a.offset >= start && a.offset <= end)
    }

    /// Get the current version
    pub fn version(&self) -> u64 {
        self.version
    }
}

/// A range defined by two anchors (start and end)
///
/// Useful for selections, diagnostics, or any span that should survive edits.
#[derive(Clone, Debug)]
pub struct AnchorRange {
    /// Start of the range (typically with Left bias)
    pub start: Anchor,
    /// End of the range (typically with Right bias)
    pub end: Anchor,
}

impl AnchorRange {
    /// Create a new anchor range
    pub fn new(start: usize, end: usize) -> Self {
        Self {
            start: Anchor::at(start),
            end: Anchor::at_right(end),
        }
    }

    /// Create a range with custom anchors
    pub fn from_anchors(start: Anchor, end: Anchor) -> Self {
        Self { start, end }
    }

    /// Get the start offset
    pub fn start_offset(&self) -> usize {
        self.start.offset
    }

    /// Get the end offset
    pub fn end_offset(&self) -> usize {
        self.end.offset
    }

    /// Get the range as a tuple (start, end)
    pub fn as_tuple(&self) -> (usize, usize) {
        let s = self.start.offset;
        let e = self.end.offset;
        if s <= e { (s, e) } else { (e, s) }
    }

    /// Check if the range is empty (start == end)
    pub fn is_empty(&self) -> bool {
        self.start.offset == self.end.offset
    }

    /// Check if a position is within this range
    pub fn contains(&self, offset: usize) -> bool {
        let (start, end) = self.as_tuple();
        offset >= start && offset < end
    }

    /// Adjust this range based on an edit
    pub fn adjust(&mut self, edit: &TextEdit) {
        self.start.offset = AnchorSet::adjust_offset(self.start.offset, self.start.bias, edit);
        self.end.offset = AnchorSet::adjust_offset(self.end.offset, self.end.bias, edit);
    }
}

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
        self.start().cmp(&other.start())
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
        self.selections.iter().map(|s| {
            if s.is_cursor() {
                (s.head_offset(), None)
            } else {
                (s.head_offset(), Some(s.anchor_offset()))
            }
        }).collect()
    }

    /// Create from the legacy Cursor format
    pub fn from_cursors(cursors: &[Cursor]) -> Self {
        let selections: Vec<Selection> = cursors.iter().map(|c| {
            if let Some(anchor) = c.anchor {
                Selection::new(c.position, anchor)
            } else {
                Selection::cursor(c.position)
            }
        }).collect();

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
        self.selections.iter().map(|s| {
            if s.is_cursor() {
                Cursor::new(s.head_offset())
            } else {
                Cursor::with_selection(s.head_offset(), s.anchor_offset())
            }
        }).collect()
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
        self.anchor.map(|a| a.min(self.position)).unwrap_or(self.position)
    }

    /// Get the end of the selection (or cursor position if no selection)
    pub fn selection_end(&self) -> usize {
        self.anchor.map(|a| a.max(self.position)).unwrap_or(self.position)
    }
}

/// Type of edit for transaction grouping decisions
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    /// Inserting characters (typing)
    Insert,
    /// Deleting backward (backspace)
    DeleteBackward,
    /// Deleting forward (delete key)
    DeleteForward,
    /// Inserting a newline (breaks transaction)
    Newline,
    /// Pasting text (always its own transaction)
    Paste,
    /// Other operations that should be their own transaction
    Other,
}

/// A single edit operation that can be undone/redone
#[derive(Clone, Debug)]
pub struct EditOperation {
    /// The text that was removed (empty for insertions)
    pub removed_text: String,
    /// The text that was inserted (empty for deletions)
    pub inserted_text: String,
    /// The position where the edit occurred (char index)
    pub position: usize,
    /// Cursor position before the edit
    pub cursor_before: usize,
    /// Cursor position after the edit
    pub cursor_after: usize,
    /// The kind of edit (for grouping decisions)
    pub kind: EditKind,
}

/// A transaction groups multiple edits that should be undone/redone together
#[derive(Clone, Debug)]
pub struct EditTransaction {
    /// The operations in this transaction (in order of execution)
    pub operations: Vec<EditOperation>,
    /// When this transaction was created
    pub timestamp: Instant,
}

impl EditTransaction {
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
            timestamp: Instant::now(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }
}

impl Default for EditTransaction {
    fn default() -> Self {
        Self::new()
    }
}

/// History manager for undo/redo operations
#[derive(Clone, Debug)]
pub struct EditHistory {
    /// Stack of undo transactions
    pub undo_stack: Vec<EditTransaction>,
    /// Stack of redo transactions
    pub redo_stack: Vec<EditTransaction>,
    /// Current transaction being built (groups rapid edits)
    pub current_transaction: Option<EditTransaction>,
    /// Time interval for grouping edits into transactions (in milliseconds)
    pub group_interval_ms: u64,
    /// Maximum number of transactions to keep
    pub max_history_size: usize,
}

impl Default for EditHistory {
    fn default() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            current_transaction: None,
            group_interval_ms: 300, // Group edits within 300ms
            max_history_size: 1000,
        }
    }
}

impl EditHistory {
    /// Record an edit operation
    pub fn record(&mut self, operation: EditOperation) {
        let now = Instant::now();
        let op_kind = operation.kind;

        // Check if we should start a new transaction based on multiple criteria:
        // 1. Time elapsed since last edit
        // 2. Edit kind changed (e.g., typing -> deleting)
        // 3. Certain edit kinds always break transactions (newline, paste)
        // 4. Non-contiguous edits (cursor jumped)
        let should_start_new = match &self.current_transaction {
            Some(tx) => {
                let elapsed = now.duration_since(tx.timestamp).as_millis() as u64;

                // Time-based break
                if elapsed > self.group_interval_ms {
                    return self.start_new_transaction(operation, now);
                }

                // Certain operations always start a new transaction
                if matches!(op_kind, EditKind::Newline | EditKind::Paste | EditKind::Other) {
                    return self.start_new_transaction(operation, now);
                }

                // Check if edit kind changed (typing vs deleting)
                if let Some(last_op) = tx.operations.last() {
                    let last_kind = last_op.kind;

                    // Different edit directions should break transaction
                    let kind_changed = match (last_kind, op_kind) {
                        (EditKind::Insert, EditKind::Insert) => false,
                        (EditKind::DeleteBackward, EditKind::DeleteBackward) => false,
                        (EditKind::DeleteForward, EditKind::DeleteForward) => false,
                        _ => true,
                    };

                    if kind_changed {
                        return self.start_new_transaction(operation, now);
                    }

                    // Check for non-contiguous edits
                    // For inserts: new position should be right after last cursor_after
                    // For deletes: position should be adjacent to last position
                    let is_contiguous = match op_kind {
                        EditKind::Insert => operation.position == last_op.cursor_after,
                        EditKind::DeleteBackward => operation.cursor_before == last_op.cursor_after,
                        EditKind::DeleteForward => operation.position == last_op.position,
                        _ => false,
                    };

                    if !is_contiguous {
                        return self.start_new_transaction(operation, now);
                    }
                }

                false
            }
            None => true,
        };

        if should_start_new {
            self.start_new_transaction(operation, now);
        } else {
            // Add to current transaction and update timestamp
            if let Some(tx) = &mut self.current_transaction {
                tx.operations.push(operation);
                tx.timestamp = now; // Update timestamp for continued grouping
            }
        }

        // Clear redo stack on new edit
        self.redo_stack.clear();
    }

    /// Helper to start a new transaction
    fn start_new_transaction(&mut self, operation: EditOperation, timestamp: Instant) {
        // Finalize current transaction if exists
        self.finalize_transaction();
        // Start new transaction
        self.current_transaction = Some(EditTransaction {
            operations: vec![operation],
            timestamp,
        });
        // Clear redo stack on new edit
        self.redo_stack.clear();
    }

    /// Finalize the current transaction and push to undo stack
    pub fn finalize_transaction(&mut self) {
        if let Some(tx) = self.current_transaction.take() {
            if !tx.is_empty() {
                self.undo_stack.push(tx);
                // Trim history if needed
                while self.undo_stack.len() > self.max_history_size {
                    self.undo_stack.remove(0);
                }
            }
        }
    }

    /// Pop a transaction from the undo stack for undoing
    pub fn pop_undo(&mut self) -> Option<EditTransaction> {
        // First finalize any pending transaction
        self.finalize_transaction();
        self.undo_stack.pop()
    }

    /// Push a transaction to the redo stack
    pub fn push_redo(&mut self, transaction: EditTransaction) {
        self.redo_stack.push(transaction);
    }

    /// Pop a transaction from the redo stack for redoing
    pub fn pop_redo(&mut self) -> Option<EditTransaction> {
        self.redo_stack.pop()
    }

    /// Push a transaction to the undo stack (used when redoing)
    pub fn push_undo(&mut self, transaction: EditTransaction) {
        self.undo_stack.push(transaction);
        // Trim history if needed
        while self.undo_stack.len() > self.max_history_size {
            self.undo_stack.remove(0);
        }
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty() || self.current_transaction.as_ref().is_some_and(|tx| !tx.is_empty())
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Clear all history
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.current_transaction = None;
    }
}

/// A segment of text with a specific color on a specific line
#[derive(Clone, Debug)]
pub struct LineSegment {
    pub text: String,
    pub color: Color,
}

// ========== Soft Line Wrapping ==========

/// A wrapped row represents a visual row in the display
/// Multiple wrapped rows can come from the same buffer line
#[derive(Clone, Debug)]
pub struct WrappedRow {
    /// The buffer line index this row comes from
    pub buffer_line: usize,
    /// The character offset within the buffer line where this row starts
    pub start_offset: usize,
    /// The character offset within the buffer line where this row ends (exclusive)
    pub end_offset: usize,
    /// Whether this is a continuation of the previous line (wrapped)
    pub is_continuation: bool,
    /// The segments for this wrapped row (with colors)
    pub segments: Vec<LineSegment>,
}

/// Display map that handles soft line wrapping
/// Maps between buffer lines and display rows
#[derive(Clone, Debug, Default)]
pub struct DisplayMap {
    /// All wrapped rows in display order
    pub rows: Vec<WrappedRow>,
    /// Wrap width in characters (0 = no wrapping)
    pub wrap_width: usize,
    /// Version counter to track when map needs rebuilding
    pub version: u64,
}

impl DisplayMap {
    /// Create a new display map with the given wrap width
    pub fn new(wrap_width: usize) -> Self {
        Self {
            rows: Vec::new(),
            wrap_width,
            version: 0,
        }
    }

    /// Clear all rows
    pub fn clear(&mut self) {
        self.rows.clear();
        self.version += 1;
    }

    /// Get the total number of display rows
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Convert a buffer position (line, column) to display position (row, column)
    pub fn buffer_to_display(&self, buffer_line: usize, buffer_col: usize) -> (usize, usize) {
        let mut display_row = 0;

        for row in &self.rows {
            if row.buffer_line == buffer_line {
                if buffer_col >= row.start_offset && buffer_col < row.end_offset {
                    return (display_row, buffer_col - row.start_offset);
                } else if buffer_col < row.start_offset {
                    // Column is before this row, must be on previous row
                    return (display_row.saturating_sub(1), buffer_col);
                }
            } else if row.buffer_line > buffer_line {
                // We've passed the target line
                break;
            }
            display_row += 1;
        }

        // Fallback: return last row of the buffer line
        (display_row.saturating_sub(1), buffer_col)
    }

    /// Convert a display position (row, column) to buffer position (line, column)
    pub fn display_to_buffer(&self, display_row: usize, display_col: usize) -> (usize, usize) {
        if let Some(row) = self.rows.get(display_row) {
            let buffer_col = row.start_offset + display_col;
            (row.buffer_line, buffer_col.min(row.end_offset.saturating_sub(1)))
        } else if let Some(last_row) = self.rows.last() {
            (last_row.buffer_line, last_row.end_offset)
        } else {
            (0, 0)
        }
    }

    /// Get the display row for a buffer line (first row of that line)
    pub fn buffer_line_to_first_row(&self, buffer_line: usize) -> usize {
        for (i, row) in self.rows.iter().enumerate() {
            if row.buffer_line == buffer_line && !row.is_continuation {
                return i;
            }
        }
        self.rows.len().saturating_sub(1)
    }

    /// Get the buffer line for a display row
    pub fn row_to_buffer_line(&self, display_row: usize) -> usize {
        self.rows.get(display_row).map(|r| r.buffer_line).unwrap_or(0)
    }

    /// Check if a display row is a continuation (wrapped) row
    pub fn is_continuation(&self, display_row: usize) -> bool {
        self.rows.get(display_row).map(|r| r.is_continuation).unwrap_or(false)
    }

    /// Build the display map from buffer lines
    pub fn rebuild(
        &mut self,
        lines: &[Vec<LineSegment>],
        wrap_width: usize,
        _char_width: f32,
    ) {
        self.rows.clear();
        self.wrap_width = wrap_width;

        if wrap_width == 0 {
            // No wrapping - each buffer line is one display row
            for (line_idx, segments) in lines.iter().enumerate() {
                let total_chars: usize = segments.iter().map(|s| s.text.chars().count()).sum();
                self.rows.push(WrappedRow {
                    buffer_line: line_idx,
                    start_offset: 0,
                    end_offset: total_chars,
                    is_continuation: false,
                    segments: segments.clone(),
                });
            }
        } else {
            // Wrap lines at wrap_width characters
            for (line_idx, segments) in lines.iter().enumerate() {
                self.wrap_line(line_idx, segments, wrap_width);
            }
        }

        self.version += 1;
    }

    /// Wrap a single line into multiple rows
    fn wrap_line(&mut self, buffer_line: usize, segments: &[LineSegment], wrap_width: usize) {
        // Collect all text and track segment boundaries
        let mut all_text = String::new();
        let mut segment_boundaries: Vec<(usize, Color)> = Vec::new();
        let mut current_pos = 0;

        for seg in segments {
            segment_boundaries.push((current_pos, seg.color));
            all_text.push_str(&seg.text);
            current_pos += seg.text.chars().count();
        }

        let total_chars = all_text.chars().count();

        if total_chars == 0 {
            // Empty line
            self.rows.push(WrappedRow {
                buffer_line,
                start_offset: 0,
                end_offset: 0,
                is_continuation: false,
                segments: vec![],
            });
            return;
        }

        let chars: Vec<char> = all_text.chars().collect();
        let mut start = 0;
        let mut is_first_row = true;

        while start < total_chars {
            // Find where to break
            let mut end = (start + wrap_width).min(total_chars);

            // Try to break at word boundary (space) if not at end
            if end < total_chars && wrap_width > 0 {
                // Look backwards for a space to break at
                let search_start = start;
                let mut break_pos = end;
                for i in (search_start..end).rev() {
                    if chars[i] == ' ' || chars[i] == '\t' {
                        break_pos = i + 1; // Break after the space
                        break;
                    }
                }
                // Only use word break if it's not too far back (at least half the wrap width)
                if break_pos > start + wrap_width / 2 {
                    end = break_pos;
                }
            }

            // Build segments for this row
            let row_segments = self.build_row_segments(&chars, start, end, &segment_boundaries);

            self.rows.push(WrappedRow {
                buffer_line,
                start_offset: start,
                end_offset: end,
                is_continuation: !is_first_row,
                segments: row_segments,
            });

            start = end;
            is_first_row = false;
        }
    }

    /// Build segments for a row, respecting color boundaries
    fn build_row_segments(
        &self,
        chars: &[char],
        start: usize,
        end: usize,
        segment_boundaries: &[(usize, Color)],
    ) -> Vec<LineSegment> {
        let mut result = Vec::new();
        let mut current_pos = start;

        while current_pos < end {
            // Find which segment we're in
            let mut seg_color = Color::WHITE;
            let mut seg_end = end;

            for (i, (boundary_start, color)) in segment_boundaries.iter().enumerate() {
                if *boundary_start <= current_pos {
                    seg_color = *color;
                    // Find where this segment ends
                    if let Some((next_boundary, _)) = segment_boundaries.get(i + 1) {
                        seg_end = (*next_boundary).min(end);
                    } else {
                        seg_end = end;
                    }
                }
                if *boundary_start > current_pos {
                    seg_end = (*boundary_start).min(end);
                    break;
                }
            }

            // Extract the text for this segment
            let text: String = chars[current_pos..seg_end].iter().collect();
            if !text.is_empty() {
                result.push(LineSegment {
                    text,
                    color: seg_color,
                });
            }

            current_pos = seg_end;
        }

        result
    }
}

/// Token with its highlight type and text content
#[derive(Clone, Debug)]
pub struct HighlightedToken {
    pub text: String,
    pub highlight_type: Option<String>,
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

    /// Horizontal offset for the editor content (useful for sidebars)
    pub offset_x: f32,

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

impl Default for ViewportDimensions {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            offset_x: 0.0,
            // Default layout values (can be overridden by UI plugin)
            text_area_left: 80.0,
            text_area_top: 10.0,
            gutter_width: 60.0,
            separator_x: 70.0,
        }
    }
}

/// Main editor state resource
#[derive(Resource)]
pub struct CodeEditorState {
    /// Text buffer (efficient rope data structure)
    pub rope: Rope,

    /// Cursor position (char index) - primary cursor for backward compatibility
    pub cursor_pos: usize,

    /// Last cursor position (for detecting cursor movement)
    pub last_cursor_pos: usize,

    /// Selection start (None = no selection) - primary cursor for backward compatibility
    pub selection_start: Option<usize>,

    /// Selection end - primary cursor for backward compatibility
    pub selection_end: Option<usize>,

    /// All cursors (including primary cursor at index 0)
    /// The first cursor is the "primary" cursor that maps to cursor_pos/selection_start/selection_end
    pub cursors: Vec<Cursor>,

    /// Is editor focused
    pub is_focused: bool,

    /// Needs full re-render
    pub needs_update: bool,

    /// Only scroll changed, don't rebuild entities
    pub needs_scroll_update: bool,

    /// Cached highlighted tokens
    pub tokens: Vec<HighlightedToken>,

    /// Cached processed lines for rendering (optimization)
    pub lines: Vec<Vec<LineSegment>>,

    /// Vertical scroll offset in pixels
    pub scroll_offset: f32,

    /// Target vertical scroll offset for smooth scrolling
    pub target_scroll_offset: f32,

    /// Horizontal scroll offset in pixels
    pub horizontal_scroll_offset: f32,

    /// Target horizontal scroll offset for smooth scrolling
    pub target_horizontal_scroll_offset: f32,

    /// Maximum content width (longest line in pixels)
    pub max_content_width: f32,

    /// Version when max_content_width was last calculated (PERFORMANCE)
    /// Used to avoid recalculating on every frame
    pub max_content_width_version: u64,

    /// The line index that has the max width (PERFORMANCE)
    /// If this line is edited, we need to recalculate max width
    pub max_width_line: Option<usize>,

    /// Pool of reusable text entities (PERFORMANCE)
    pub entity_pool: Vec<Entity>,

    /// Pool of reusable line number entities (PERFORMANCE)
    pub line_number_pool: Vec<Entity>,

    /// Track which lines changed for incremental highlighting (PERFORMANCE)
    pub dirty_lines: Option<Range<usize>>,

    /// Track line count for detecting changes
    pub previous_line_count: usize,

    /// Content version - incremented on every text change (for skipping re-highlight on cursor-only moves)
    pub content_version: u64,

    /// Last content version when highlighting was run
    pub last_highlighted_version: u64,

    /// Last content version when line segments were built (PERFORMANCE)
    pub last_lines_version: u64,

    /// Last syntax tree version that was rendered (PERFORMANCE)
    #[cfg(feature = "tree-sitter")]
    pub last_rendered_tree_version: u64,

    /// Debouncing: true if update is pending but not yet applied (PERFORMANCE)
    pub pending_update: bool,

    /// Last time we rendered (in seconds) for debouncing (PERFORMANCE)
    pub last_render_time: f64,

    /// Edit history for undo/redo
    pub history: EditHistory,

    /// Anchor set for edit-resilient position tracking
    /// Use this for positions that need to survive text edits (bookmarks, diagnostics, etc.)
    pub anchors: AnchorSet,

    /// Selection collection for managing multiple selections with edit-awareness
    /// This is the modern replacement for the legacy `cursors` field, providing:
    /// - Automatic sorting and merging of overlapping selections
    /// - Edit-resilient positions via anchors
    /// - Unified interface for single and multi-cursor operations
    pub selections: SelectionCollection,

    /// Display map for soft line wrapping
    /// Maps buffer lines to display rows when wrapping is enabled
    pub display_map: DisplayMap,

    /// Line width tracker for O(log n) max line width queries
    /// Used for horizontal scrolling bounds calculation
    pub line_width_tracker: LineWidthTracker,

    /// Pending text edit for tree-sitter incremental parsing
    /// Stores byte positions of the most recent edit to be sent as an event
    /// Format: (start_byte, old_end_byte, new_end_byte)
    #[cfg(feature = "tree-sitter")]
    pub pending_tree_sitter_edit: Option<(usize, usize, usize)>,

}

impl Default for CodeEditorState {
    fn default() -> Self {
        let initial_text = "";
        let rope = Rope::from_str(initial_text);
        let line_count = rope.len_lines();

        Self {
            rope,
            cursor_pos: 0,
            last_cursor_pos: 0,
            selection_start: None,
            selection_end: None,
            cursors: vec![Cursor::new(0)],
            is_focused: false,
            needs_update: true,
            needs_scroll_update: false,
            tokens: Vec::new(),
            lines: Vec::new(),
            scroll_offset: 0.0,
            target_scroll_offset: 0.0,
            horizontal_scroll_offset: 0.0,
            target_horizontal_scroll_offset: 0.0,
            max_content_width: 0.0,
            max_content_width_version: 0,
            max_width_line: None,
            entity_pool: Vec::new(),
            line_number_pool: Vec::new(),
            dirty_lines: None,
            previous_line_count: line_count,
            content_version: 0,
            last_highlighted_version: u64::MAX, // Force initial highlighting
            last_lines_version: 0,
            #[cfg(feature = "tree-sitter")]
            last_rendered_tree_version: 0,
            pending_update: false,
            last_render_time: 0.0,
            history: EditHistory::default(),
            anchors: AnchorSet::new(),
            selections: SelectionCollection::new(),
            display_map: DisplayMap::default(),
            line_width_tracker: LineWidthTracker::new(),
            #[cfg(feature = "tree-sitter")]
            pending_tree_sitter_edit: None,
        }
    }
}

impl CodeEditorState {
    /// Create new editor state with initial text
    pub fn new(text: &str) -> Self {
        let rope = Rope::from_str(text);
        let line_count = rope.len_lines();
        let line_width_tracker = LineWidthTracker::from_rope(&rope);

        Self {
            rope,
            cursor_pos: 0,
            last_cursor_pos: 0,
            selection_start: None,
            selection_end: None,
            cursors: vec![Cursor::new(0)],
            is_focused: false,
            needs_update: true,
            needs_scroll_update: false,
            tokens: Vec::new(),
            lines: Vec::new(),
            scroll_offset: 0.0,
            target_scroll_offset: 0.0,
            horizontal_scroll_offset: 0.0,
            target_horizontal_scroll_offset: 0.0,
            max_content_width: 0.0,
            max_content_width_version: 0,
            max_width_line: None,
            entity_pool: Vec::new(),
            line_number_pool: Vec::new(),
            dirty_lines: None,
            previous_line_count: line_count,
            content_version: 0,
            last_highlighted_version: u64::MAX, // Force initial highlighting
            last_lines_version: 0,
            #[cfg(feature = "tree-sitter")]
            last_rendered_tree_version: 0,
            pending_update: false,
            last_render_time: 0.0,
            history: EditHistory::default(),
            anchors: AnchorSet::new(),
            selections: SelectionCollection::new(),
            display_map: DisplayMap::default(),
            line_width_tracker,
            #[cfg(feature = "tree-sitter")]
            pending_tree_sitter_edit: None,
        }
    }

    /// Get text content as string
    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    /// Get line count
    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    /// Insert character at cursor position (with undo recording)
    pub fn insert_char(&mut self, c: char) {
        self.insert_char_with_history(c, true);
    }

    /// Insert character at cursor position with optional history recording
    pub fn insert_char_with_history(&mut self, c: char, record_history: bool) {
        let cursor_pos = self.cursor_pos.min(self.rope.len_chars());
        let line_idx = self.rope.char_to_line(cursor_pos);
        let cursor_before = cursor_pos;

        // Record byte positions for tree-sitter incremental parsing
        #[cfg(feature = "tree-sitter")]
        let start_byte = self.rope.char_to_byte(cursor_pos);
        #[cfg(feature = "tree-sitter")]
        let char_byte_len = c.len_utf8();

        // Record anchor edit (character-based, not byte-based)
        // Note: We only record edits to anchors, not selections, because selections
        // are synced from cursor_pos which is updated directly by text operations
        self.anchors.record_edit(TextEdit::insert(cursor_pos, 1));

        self.rope.insert_char(cursor_pos, c);
        self.cursor_pos += 1;
        self.sync_cursors_from_primary();
        // Mark for update with debouncing (avoids rebuilding mesh on every keystroke)
        self.pending_update = true;
        self.content_version += 1;

        // Record edit for tree-sitter incremental parsing
        #[cfg(feature = "tree-sitter")]
        {
            self.pending_tree_sitter_edit = Some((
                start_byte,
                start_byte,  // old_end = start for insertion
                start_byte + char_byte_len,  // new_end = start + inserted bytes
            ));
        }

        // Record for undo
        if record_history {
            let kind = if c == '\n' { EditKind::Newline } else { EditKind::Insert };
            self.history.record(EditOperation {
                removed_text: String::new(),
                inserted_text: c.to_string(),
                position: cursor_before,
                cursor_before,
                cursor_after: self.cursor_pos,
                kind,
            });
        }

        let new_line_count = self.rope.len_lines();
        if c == '\n' {
            self.dirty_lines = Some(line_idx..new_line_count);
        } else {
            self.dirty_lines = Some(line_idx..(line_idx + 1).min(new_line_count));
        }
        self.previous_line_count = new_line_count;
    }

    /// Delete character before cursor (with undo recording)
    pub fn delete_backward(&mut self) {
        self.delete_backward_with_history(true);
    }

    /// Delete character before cursor with optional history recording
    pub fn delete_backward_with_history(&mut self, record_history: bool) {
        if self.cursor_pos > 0 && self.cursor_pos <= self.rope.len_chars() {
            let cursor_before = self.cursor_pos;
            let line_idx = self.rope.char_to_line(self.cursor_pos - 1);

            // Get the character being deleted
            let deleted_char = self.rope.char(self.cursor_pos - 1);

            let char_idx = self.rope.char_to_byte(self.cursor_pos - 1);
            let byte_idx_end = self.rope.char_to_byte(self.cursor_pos);

            // Record anchor edit (character-based)
            self.anchors.record_edit(TextEdit::delete(self.cursor_pos - 1, self.cursor_pos));

            self.rope.remove(char_idx..byte_idx_end);
            self.cursor_pos -= 1;
            self.sync_cursors_from_primary();
            // Mark for update with debouncing
            self.pending_update = true;
            self.content_version += 1;

            // Record edit for tree-sitter incremental parsing
            #[cfg(feature = "tree-sitter")]
            {
                self.pending_tree_sitter_edit = Some((
                    char_idx,       // start_byte
                    byte_idx_end,   // old_end = end of deleted character
                    char_idx,       // new_end = start (nothing inserted)
                ));
            }

            // Record for undo
            if record_history {
                self.history.record(EditOperation {
                    removed_text: deleted_char.to_string(),
                    inserted_text: String::new(),
                    position: self.cursor_pos,
                    cursor_before,
                    cursor_after: self.cursor_pos,
                    kind: EditKind::DeleteBackward,
                });
            }

            let new_line_count = self.rope.len_lines();
            self.dirty_lines = Some(line_idx..new_line_count);
            self.previous_line_count = new_line_count;
        }
    }

    /// Delete character after cursor (with undo recording)
    pub fn delete_forward(&mut self) {
        self.delete_forward_with_history(true);
    }

    /// Delete character after cursor with optional history recording
    pub fn delete_forward_with_history(&mut self, record_history: bool) {
        if self.cursor_pos < self.rope.len_chars() {
            let cursor_before = self.cursor_pos;
            let line_idx = self.rope.char_to_line(self.cursor_pos);

            // Get the character being deleted
            let deleted_char = self.rope.char(self.cursor_pos);

            let char_idx = self.rope.char_to_byte(self.cursor_pos);
            let byte_idx_end = self.rope.char_to_byte(self.cursor_pos + 1);

            // Record anchor edit (character-based)
            self.anchors.record_edit(TextEdit::delete(self.cursor_pos, self.cursor_pos + 1));

            self.rope.remove(char_idx..byte_idx_end);
            self.sync_cursors_from_primary();
            // Mark for update with debouncing
            self.pending_update = true;
            self.content_version += 1;

            // Record edit for tree-sitter incremental parsing
            #[cfg(feature = "tree-sitter")]
            {
                self.pending_tree_sitter_edit = Some((
                    char_idx,       // start_byte
                    byte_idx_end,   // old_end = end of deleted character
                    char_idx,       // new_end = start (nothing inserted)
                ));
            }

            // Record for undo
            if record_history {
                self.history.record(EditOperation {
                    removed_text: deleted_char.to_string(),
                    inserted_text: String::new(),
                    position: self.cursor_pos,
                    cursor_before,
                    cursor_after: self.cursor_pos,
                    kind: EditKind::DeleteForward,
                });
            }

            let new_line_count = self.rope.len_lines();
            self.dirty_lines = Some(line_idx..new_line_count);
            self.previous_line_count = new_line_count;
        }
    }

    /// Insert text at a specific position (used for undo/redo)
    pub fn insert_text_at(&mut self, pos: usize, text: &str) {
        let pos = pos.min(self.rope.len_chars());
        let text_char_len = text.chars().count();

        // Record byte positions for tree-sitter incremental parsing
        #[cfg(feature = "tree-sitter")]
        let start_byte = self.rope.char_to_byte(pos);
        #[cfg(feature = "tree-sitter")]
        let text_byte_len = text.len();

        // Record anchor edit (character-based)
        self.anchors.record_edit(TextEdit::insert(pos, text_char_len));

        self.rope.insert(pos, text);
        self.pending_update = true;
        self.content_version += 1;
        self.dirty_lines = None; // Full rehighlight
        self.previous_line_count = self.rope.len_lines();

        // Record edit for tree-sitter incremental parsing
        #[cfg(feature = "tree-sitter")]
        {
            self.pending_tree_sitter_edit = Some((
                start_byte,
                start_byte,  // old_end = start for insertion
                start_byte + text_byte_len,  // new_end = start + inserted bytes
            ));
        }
    }

    /// Remove text range (used for undo/redo)
    pub fn remove_range(&mut self, start: usize, end: usize) {
        let start = start.min(self.rope.len_chars());
        let end = end.min(self.rope.len_chars());
        if start < end {
            let start_byte = self.rope.char_to_byte(start);
            let end_byte = self.rope.char_to_byte(end);

            // Record anchor edit (character-based)
            self.anchors.record_edit(TextEdit::delete(start, end));

            self.rope.remove(start_byte..end_byte);
            self.pending_update = true;
            self.content_version += 1;
            self.dirty_lines = None; // Full rehighlight
            self.previous_line_count = self.rope.len_lines();

            // Record edit for tree-sitter incremental parsing
            #[cfg(feature = "tree-sitter")]
            {
                self.pending_tree_sitter_edit = Some((
                    start_byte,  // start_byte
                    end_byte,    // old_end = end of deleted range
                    start_byte,  // new_end = start (nothing inserted)
                ));
            }
        }
    }

    /// Perform undo operation
    pub fn undo(&mut self) -> bool {
        if let Some(transaction) = self.history.pop_undo() {
            // Apply operations in reverse order
            for op in transaction.operations.iter().rev() {
                // Undo: remove inserted text, insert removed text
                if !op.inserted_text.is_empty() {
                    let end_pos = op.position + op.inserted_text.chars().count();
                    self.remove_range(op.position, end_pos);
                }
                if !op.removed_text.is_empty() {
                    self.insert_text_at(op.position, &op.removed_text);
                }
            }

            // Restore cursor to before the first operation
            if let Some(first_op) = transaction.operations.first() {
                self.cursor_pos = first_op.cursor_before;
            }

            // Push to redo stack
            self.history.push_redo(transaction);
            true
        } else {
            false
        }
    }

    /// Perform redo operation
    pub fn redo(&mut self) -> bool {
        if let Some(transaction) = self.history.pop_redo() {
            // Apply operations in forward order
            for op in transaction.operations.iter() {
                // Redo: remove removed text (it was re-inserted by undo), insert inserted text
                if !op.removed_text.is_empty() {
                    let end_pos = op.position + op.removed_text.chars().count();
                    self.remove_range(op.position, end_pos);
                }
                if !op.inserted_text.is_empty() {
                    self.insert_text_at(op.position, &op.inserted_text);
                }
            }

            // Restore cursor to after the last operation
            if let Some(last_op) = transaction.operations.last() {
                self.cursor_pos = last_op.cursor_after;
            }

            // Push to undo stack
            self.history.push_undo(transaction);
            true
        } else {
            false
        }
    }

    /// Move cursor by delta
    pub fn move_cursor(&mut self, delta: isize) {
        if delta < 0 {
            let amount = (-delta) as usize;
            self.cursor_pos = self.cursor_pos.saturating_sub(amount);
        } else {
            let amount = delta as usize;
            self.cursor_pos = (self.cursor_pos + amount).min(self.rope.len_chars());
        }
    }

    /// Set text content
    pub fn set_text(&mut self, text: &str) {
        // Record byte length before replacement
        #[cfg(feature = "tree-sitter")]
        let old_byte_len = self.rope.len_bytes();
        #[cfg(feature = "tree-sitter")]
        let new_byte_len = text.len();

        self.rope = Rope::from_str(text);
        self.cursor_pos = self.cursor_pos.min(self.rope.len_chars());
        self.pending_update = true;
        self.content_version += 1;
        self.dirty_lines = None;
        self.previous_line_count = self.rope.len_lines();
        // Clear anchors and reset selections when text is replaced entirely
        self.anchors.clear();
        self.selections = SelectionCollection::with_cursor(self.cursor_pos);
        // Rebuild line width tracker for O(log n) max width queries
        self.line_width_tracker.rebuild(&self.rope);
        // Invalidate cached max content width
        self.max_content_width_version = 0;

        // Record edit for tree-sitter incremental parsing (full document replacement)
        #[cfg(feature = "tree-sitter")]
        {
            self.pending_tree_sitter_edit = Some((
                0,              // start_byte = beginning of document
                old_byte_len,   // old_end = old document length
                new_byte_len,   // new_end = new document length
            ));
        }
    }

    // ========== Anchor methods ==========

    /// Create an anchor at the given position with left bias
    /// Returns the anchor (caller should store it if they need to track the position)
    pub fn create_anchor(&mut self, offset: usize, bias: AnchorBias) -> Anchor {
        let offset = offset.min(self.rope.len_chars());
        self.anchors.anchor_at(offset, bias)
    }

    /// Create an anchor at the given position with left bias (cursor-like behavior)
    pub fn anchor_at(&mut self, offset: usize) -> Anchor {
        self.create_anchor(offset, AnchorBias::Left)
    }

    /// Resolve an anchor's current position (applies pending edits)
    pub fn resolve_anchor(&self, anchor: &Anchor) -> usize {
        self.anchors.resolve(anchor).min(self.rope.len_chars())
    }

    /// Apply pending anchor edits (call this periodically or before reading anchors)
    pub fn apply_anchor_edits(&mut self) {
        self.anchors.apply_pending_edits();
    }

    /// Remove an anchor by its ID
    pub fn remove_anchor(&mut self, id: u64) -> Option<Anchor> {
        self.anchors.remove(id)
    }

    // ========== SelectionCollection methods ==========

    /// Apply pending edits to the selection collection
    /// Call this periodically or before reading selection positions
    pub fn apply_selection_edits(&mut self) {
        self.selections.apply_pending_edits();
    }

    /// Sync the legacy cursor fields from the SelectionCollection
    /// Call this after modifying selections to keep legacy code working
    pub fn sync_from_selections(&mut self) {
        let primary = self.selections.primary();
        self.cursor_pos = primary.head_offset();
        if primary.has_selection() {
            self.selection_start = Some(primary.anchor_offset());
            self.selection_end = Some(primary.head_offset());
        } else {
            self.selection_start = None;
            self.selection_end = None;
        }
        // Also sync the legacy cursors Vec
        self.cursors = self.selections.to_cursors();
    }

    /// Sync the SelectionCollection from legacy cursor fields
    /// Call this when legacy code has modified cursor_pos/selection_start/selection_end
    pub fn sync_to_selections(&mut self) {
        if let Some(anchor) = self.selection_start {
            self.selections.set_selection(self.cursor_pos, anchor);
        } else {
            self.selections.set_cursor(self.cursor_pos);
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
    pub fn add_selection(&mut self, offset: usize) {
        let offset = offset.min(self.rope.len_chars());
        self.selections.add_cursor(offset);
        self.sync_from_selections();
        self.pending_update = true;
    }

    /// Add a new selection with a range
    pub fn add_selection_range(&mut self, head: usize, anchor: usize) {
        let head = head.min(self.rope.len_chars());
        let anchor = anchor.min(self.rope.len_chars());
        self.selections.add_selection_range(head, anchor);
        self.sync_from_selections();
        self.pending_update = true;
    }

    /// Clear all secondary selections, keeping only the primary
    pub fn clear_secondary_selections(&mut self) {
        self.selections.clear_secondary();
        self.sync_from_selections();
        self.pending_update = true;
    }

    /// Move the primary selection to a new position
    pub fn set_primary_selection(&mut self, head: usize, extend: bool) {
        let head = head.min(self.rope.len_chars());
        self.selections.move_primary(head, extend);
        self.sync_from_selections();
        self.pending_update = true;
    }

    // ========== Multi-cursor methods ==========

    /// Sync the primary cursor (cursor_pos/selection_start/selection_end) with cursors[0]
    pub fn sync_primary_cursor(&mut self) {
        if let Some(primary) = self.cursors.first() {
            self.cursor_pos = primary.position;
            self.selection_start = primary.anchor;
            self.selection_end = if primary.anchor.is_some() {
                Some(primary.position)
            } else {
                None
            };
        }
    }

    /// Sync cursors[0] from the primary cursor fields
    pub fn sync_cursors_from_primary(&mut self) {
        if self.cursors.is_empty() {
            self.cursors.push(Cursor::new(self.cursor_pos));
        }
        self.cursors[0].position = self.cursor_pos;
        self.cursors[0].anchor = self.selection_start;
    }

    /// Add a new cursor at the given position
    pub fn add_cursor(&mut self, position: usize) {
        let position = position.min(self.rope.len_chars());
        // Don't add duplicate cursor at same position
        if !self.cursors.iter().any(|c| c.position == position) {
            self.cursors.push(Cursor::new(position));
            self.sort_and_merge_cursors();
            self.pending_update = true;
        }
    }

    /// Add a new cursor with selection
    pub fn add_cursor_with_selection(&mut self, position: usize, anchor: usize) {
        let position = position.min(self.rope.len_chars());
        let anchor = anchor.min(self.rope.len_chars());
        self.cursors.push(Cursor::with_selection(position, anchor));
        self.sort_and_merge_cursors();
        self.pending_update = true;
    }

    /// Remove all cursors except the primary one
    pub fn clear_secondary_cursors(&mut self) {
        if !self.cursors.is_empty() {
            self.cursors.truncate(1);
        }
        self.sync_primary_cursor();
        self.pending_update = true;
    }

    /// Check if we have multiple cursors
    pub fn has_multiple_cursors(&self) -> bool {
        self.cursors.len() > 1
    }

    /// Get the number of cursors
    pub fn cursor_count(&self) -> usize {
        self.cursors.len()
    }

    /// Sort cursors by position and merge overlapping selections
    pub fn sort_and_merge_cursors(&mut self) {
        if self.cursors.len() <= 1 {
            return;
        }

        // Sort by position
        self.cursors.sort_by_key(|c| c.position);

        // Merge overlapping selections
        let mut merged: Vec<Cursor> = Vec::with_capacity(self.cursors.len());
        for cursor in self.cursors.drain(..) {
            if let Some(last) = merged.last_mut() {
                let last_end = last.selection_end();
                let cursor_start = cursor.selection_start();

                // If selections overlap or are adjacent, merge them
                if cursor_start <= last_end {
                    // Extend the last cursor's selection to include this one
                    let new_end = cursor.selection_end().max(last_end);
                    if last.anchor.is_some() || cursor.anchor.is_some() {
                        let new_start = last.selection_start().min(cursor_start);
                        last.anchor = Some(new_start);
                        last.position = new_end;
                    } else {
                        last.position = new_end;
                    }
                } else {
                    merged.push(cursor);
                }
            } else {
                merged.push(cursor);
            }
        }
        self.cursors = merged;

        // Update primary cursor from the first cursor
        self.sync_primary_cursor();
    }

    /// Find word boundaries around a position and return (start, end)
    pub fn word_at_position(&self, pos: usize) -> Option<(usize, usize)> {
        let pos = pos.min(self.rope.len_chars());
        if pos >= self.rope.len_chars() {
            return None;
        }

        let c = self.rope.char(pos);
        if !c.is_alphanumeric() && c != '_' {
            return None;
        }

        // Find start of word
        let mut start = pos;
        while start > 0 {
            let prev = self.rope.char(start - 1);
            if prev.is_alphanumeric() || prev == '_' {
                start -= 1;
            } else {
                break;
            }
        }

        // Find end of word
        let mut end = pos;
        while end < self.rope.len_chars() {
            let ch = self.rope.char(end);
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
    pub fn find_next_occurrence(&self, text: &str, after_pos: usize) -> Option<(usize, usize)> {
        if text.is_empty() {
            return None;
        }

        let text_chars: Vec<char> = text.chars().collect();
        let text_len = text_chars.len();
        let rope_len = self.rope.len_chars();

        // Search from after_pos to end
        let mut pos = after_pos;
        while pos + text_len <= rope_len {
            let mut matches = true;
            for (i, &tc) in text_chars.iter().enumerate() {
                if self.rope.char(pos + i) != tc {
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
                if self.rope.char(pos + i) != tc {
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
    pub fn add_cursor_at_next_occurrence(&mut self) -> bool {
        // Get the text to search for
        let search_text = if let Some(primary) = self.cursors.first() {
            if primary.has_selection() {
                let (start, end) = (primary.selection_start(), primary.selection_end());
                self.rope.slice(start..end).to_string()
            } else {
                // No selection - select word at cursor first
                if let Some((start, end)) = self.word_at_position(primary.position) {
                    // Select the word at the primary cursor
                    self.cursors[0] = Cursor::with_selection(end, start);
                    self.sync_primary_cursor();
                    self.pending_update = true;
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
        let search_from = self.cursors.iter()
            .map(|c| c.selection_end())
            .max()
            .unwrap_or(0);

        // Find next occurrence
        if let Some((start, end)) = self.find_next_occurrence(&search_text, search_from) {
            // Check if this position is already covered by an existing cursor
            let already_covered = self.cursors.iter().any(|c| {
                let (cs, ce) = (c.selection_start(), c.selection_end());
                start >= cs && end <= ce
            });

            if !already_covered {
                self.add_cursor_with_selection(end, start);
                return true;
            }
        }

        false
    }

    /// Record a text edit for incremental parsing (sends TextEditEvent)
    ///
    /// This method is a compatibility stub for code that previously called tree-sitter's
    /// record_edit(). The actual event sending happens at the plugin layer.
    ///
    /// NOTE: This is kept as a no-op stub because CodeEditorState is a Resource, not a System,
    /// so it cannot send Bevy events. The actual TextEditEvent is sent by watching
    /// `content_version` changes in the plugin systems.
    pub fn record_edit(&mut self, _start_byte: usize, _old_end_byte: usize, _new_end_byte: usize) {
        // No-op: Event sending happens in plugin layer by detecting content_version changes
        // This method exists only for backwards compatibility with existing code
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
#[derive(Resource)]
#[derive(Default)]
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

/// A single search match
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FindMatch {
    /// Start position (char index)
    pub start: usize,
    /// End position (char index)
    pub end: usize,
}

/// Resource to track find/search state
#[derive(Resource, Clone, Debug)]
#[derive(Default)]
pub struct FindState {
    /// Whether find mode is active
    pub active: bool,
    /// The search query
    pub query: String,
    /// All matches in the document
    pub matches: Vec<FindMatch>,
    /// Index of the currently selected match
    pub current_match_index: Option<usize>,
    /// Case-sensitive search
    pub case_sensitive: bool,
    /// Use regex search
    pub use_regex: bool,
    /// Whole word matching
    pub whole_word: bool,
}


impl FindState {
    /// Find all matches in the given rope
    pub fn search(&mut self, rope: &Rope) {
        self.matches.clear();
        self.current_match_index = None;

        if self.query.is_empty() {
            return;
        }

        let query_len_chars = self.query.chars().count();
        let total_chars = rope.len_chars();

        // Prepare query for case-insensitive comparison if needed
        let query_chars: Vec<char> = if self.case_sensitive {
            self.query.chars().collect()
        } else {
            self.query.to_lowercase().chars().collect()
        };

        // Iterate character by character through the rope
        let mut char_idx = 0;
        while char_idx + query_len_chars <= total_chars {
            // Check if query matches at this position
            let mut matches = true;
            for (q_idx, q_char) in query_chars.iter().enumerate() {
                let rope_char = rope.char(char_idx + q_idx);
                let cmp_char = if self.case_sensitive {
                    rope_char
                } else {
                    rope_char.to_lowercase().next().unwrap_or(rope_char)
                };

                if cmp_char != *q_char {
                    matches = false;
                    break;
                }
            }

            if matches {
                let start_char = char_idx;
                let end_char = char_idx + query_len_chars;

                // Check whole word if enabled
                let is_whole_word = if self.whole_word {
                    let before_ok = start_char == 0 || {
                        let prev_char = rope.char(start_char - 1);
                        !prev_char.is_alphanumeric() && prev_char != '_'
                    };
                    let after_ok = end_char >= total_chars || {
                        let next_char = rope.char(end_char);
                        !next_char.is_alphanumeric() && next_char != '_'
                    };
                    before_ok && after_ok
                } else {
                    true
                };

                if is_whole_word {
                    self.matches.push(FindMatch {
                        start: start_char,
                        end: end_char,
                    });
                }
            }

            char_idx += 1;
        }

        // Select first match if any
        if !self.matches.is_empty() {
            self.current_match_index = Some(0);
        }
    }

    /// Find the next match from the current cursor position
    pub fn find_next(&mut self, cursor_pos: usize) {
        if self.matches.is_empty() {
            return;
        }

        // Find the first match after cursor
        for (i, m) in self.matches.iter().enumerate() {
            if m.start > cursor_pos {
                self.current_match_index = Some(i);
                return;
            }
        }

        // Wrap around to first match
        self.current_match_index = Some(0);
    }

    /// Find the previous match from the current cursor position
    pub fn find_previous(&mut self, cursor_pos: usize) {
        if self.matches.is_empty() {
            return;
        }

        // Find the last match before cursor
        for (i, m) in self.matches.iter().enumerate().rev() {
            if m.end <= cursor_pos {
                self.current_match_index = Some(i);
                return;
            }
        }

        // Wrap around to last match
        self.current_match_index = Some(self.matches.len() - 1);
    }

    /// Get the current match
    pub fn current_match(&self) -> Option<FindMatch> {
        self.current_match_index.and_then(|i| self.matches.get(i).copied())
    }

    /// Clear the search
    pub fn clear(&mut self) {
        self.active = false;
        self.query.clear();
        self.matches.clear();
        self.current_match_index = None;
    }
}

/// State for "Go to line" functionality
#[derive(Clone, Debug, Default, Resource)]
pub struct GotoLineState {
    /// Whether the goto line dialog is active
    pub active: bool,
    /// The line number input (as string for easier input handling)
    pub input: String,
}

impl GotoLineState {
    /// Try to parse the input as a line number and return it (1-indexed)
    pub fn parse_line_number(&self) -> Option<usize> {
        self.input.trim().parse::<usize>().ok()
    }

    /// Execute goto line: moves cursor to the specified line
    /// Returns true if the navigation was successful
    pub fn goto(&self, state: &mut CodeEditorState) -> bool {
        if let Some(line_num) = self.parse_line_number() {
            let total_lines = state.rope.len_lines();
            // Clamp line number to valid range (1-indexed input, convert to 0-indexed)
            let target_line = line_num.saturating_sub(1).min(total_lines.saturating_sub(1));

            // Move cursor to the start of the target line
            let char_pos = state.rope.line_to_char(target_line);
            state.cursor_pos = char_pos;
            state.selection_start = None;
            state.selection_end = None;
            state.pending_update = true;

            return true;
        }
        false
    }

    /// Clear the goto line state
    pub fn clear(&mut self) {
        self.active = false;
        self.input.clear();
    }
}

/// Represents a foldable region in the code
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoldRegion {
    /// Start line of the foldable region (0-indexed)
    pub start_line: usize,
    /// End line of the foldable region (0-indexed, inclusive)
    pub end_line: usize,
    /// Whether this region is currently folded
    pub is_folded: bool,
    /// The kind of fold (function, class, block, etc.)
    pub kind: FoldKind,
    /// Indentation level (for nested folds)
    pub indent_level: usize,
}

impl FoldRegion {
    /// Create a new fold region
    pub fn new(start_line: usize, end_line: usize, kind: FoldKind) -> Self {
        Self {
            start_line,
            end_line,
            is_folded: false,
            kind,
            indent_level: 0,
        }
    }

    /// Check if this region contains a given line
    pub fn contains_line(&self, line: usize) -> bool {
        line >= self.start_line && line <= self.end_line
    }

    /// Check if this fold hides a given line (folded and line is inside but not the first)
    pub fn hides_line(&self, line: usize) -> bool {
        self.is_folded && line > self.start_line && line <= self.end_line
    }

    /// Get the number of lines this region spans
    pub fn line_count(&self) -> usize {
        self.end_line.saturating_sub(self.start_line) + 1
    }

    /// Get the number of hidden lines when folded
    pub fn hidden_line_count(&self) -> usize {
        if self.is_folded {
            self.end_line.saturating_sub(self.start_line)
        } else {
            0
        }
    }
}

/// The kind of foldable region
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FoldKind {
    /// Function or method definition
    Function,
    /// Class or struct definition
    Class,
    /// Generic block (if/else, loop, etc.)
    Block,
    /// Import/include statements
    Imports,
    /// Comment block
    Comment,
    /// Region marker (manual fold markers like #region)
    Region,
    /// Array or object literal
    Literal,
    /// Unknown/other
    Other,
}

impl FoldKind {
    /// Get the fold indicator character for the gutter
    pub fn indicator(&self) -> char {
        match self {
            FoldKind::Function => 'ƒ',
            FoldKind::Class => '◆',
            FoldKind::Comment => '/',
            _ => '▶',
        }
    }
}

/// Resource to track all fold regions and their state
#[derive(Resource, Clone, Debug)]
pub struct FoldState {
    /// All detected fold regions, sorted by start line
    pub regions: Vec<FoldRegion>,
    /// Version of the content when folds were last computed
    /// Initialized to usize::MAX to force detection on first run
    pub content_version: usize,
    /// Whether fold detection is enabled
    pub enabled: bool,
}

impl Default for FoldState {
    fn default() -> Self {
        Self {
            regions: Vec::new(),
            // Use usize::MAX as sentinel to force first detection
            content_version: usize::MAX,
            enabled: true,
        }
    }
}

impl FoldState {
    /// Create a new empty fold state
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear all fold regions
    pub fn clear(&mut self) {
        self.regions.clear();
    }

    /// Add a fold region, maintaining sorted order by start_line
    pub fn add_region(&mut self, region: FoldRegion) {
        // Find insertion point to maintain sorted order
        let pos = self.regions
            .iter()
            .position(|r| r.start_line > region.start_line)
            .unwrap_or(self.regions.len());
        self.regions.insert(pos, region);
    }

    /// Get the fold region that starts at the given line
    pub fn region_at_line(&self, line: usize) -> Option<&FoldRegion> {
        self.regions.iter().find(|r| r.start_line == line)
    }

    /// Get a mutable reference to the fold region that starts at the given line
    pub fn region_at_line_mut(&mut self, line: usize) -> Option<&mut FoldRegion> {
        self.regions.iter_mut().find(|r| r.start_line == line)
    }

    /// Toggle the fold state of the region at the given line
    pub fn toggle_fold_at_line(&mut self, line: usize) -> bool {
        if let Some(region) = self.region_at_line_mut(line) {
            region.is_folded = !region.is_folded;
            true
        } else {
            false
        }
    }

    /// Fold the region at the given line
    pub fn fold_at_line(&mut self, line: usize) -> bool {
        if let Some(region) = self.region_at_line_mut(line) {
            if !region.is_folded {
                region.is_folded = true;
                return true;
            }
        }
        false
    }

    /// Unfold the region at the given line
    pub fn unfold_at_line(&mut self, line: usize) -> bool {
        if let Some(region) = self.region_at_line_mut(line) {
            if region.is_folded {
                region.is_folded = false;
                return true;
            }
        }
        false
    }

    /// Check if a line is hidden by any fold
    pub fn is_line_hidden(&self, line: usize) -> bool {
        self.regions.iter().any(|r| r.hides_line(line))
    }

    /// Check if a line is the start of a foldable region
    pub fn is_foldable_line(&self, line: usize) -> bool {
        self.regions.iter().any(|r| r.start_line == line)
    }

    /// Check if a line is the start of a folded region
    pub fn is_folded_line(&self, line: usize) -> bool {
        self.regions.iter().any(|r| r.start_line == line && r.is_folded)
    }

    /// Fold all regions
    pub fn fold_all(&mut self) {
        for region in &mut self.regions {
            region.is_folded = true;
        }
    }

    /// Unfold all regions
    pub fn unfold_all(&mut self) {
        for region in &mut self.regions {
            region.is_folded = false;
        }
    }

    /// Fold all regions at a specific level (0 = top-level functions/classes)
    pub fn fold_level(&mut self, level: usize) {
        for region in &mut self.regions {
            if region.indent_level == level {
                region.is_folded = true;
            }
        }
    }

    /// Get total number of hidden lines
    pub fn total_hidden_lines(&self) -> usize {
        self.regions.iter()
            .filter(|r| r.is_folded)
            .map(|r| r.hidden_line_count())
            .sum()
    }

    /// Convert a display line number to actual line number (accounting for folds)
    pub fn display_to_actual_line(&self, display_line: usize) -> usize {
        let mut actual = 0;
        let mut display = 0;

        while display < display_line {
            if !self.is_line_hidden(actual) {
                display += 1;
            }
            actual += 1;
        }

        // Skip any hidden lines at the target
        while self.is_line_hidden(actual) {
            actual += 1;
        }

        actual
    }

    /// Convert an actual line number to display line number (accounting for folds)
    pub fn actual_to_display_line(&self, actual_line: usize) -> usize {
        let mut display = 0;
        for line in 0..actual_line {
            if !self.is_line_hidden(line) {
                display += 1;
            }
        }
        display
    }

    /// Get the innermost fold region containing a line (for nested folds)
    pub fn innermost_region_containing(&self, line: usize) -> Option<&FoldRegion> {
        self.regions.iter()
            .filter(|r| r.contains_line(line))
            .max_by_key(|r| r.start_line) // The one starting latest is the innermost
    }

    /// Unfold any regions that hide the given line (to reveal it)
    pub fn reveal_line(&mut self, line: usize) {
        for region in &mut self.regions {
            if region.hides_line(line) {
                region.is_folded = false;
            }
        }
    }
}

/// Component marker for fold gutter indicator entities
#[derive(Component)]
pub struct FoldIndicator {
    /// The line this indicator is for
    pub line_index: usize,
}

// ========== Editor Events ==========

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
