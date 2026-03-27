//! Anchor-based position tracking types

use std::sync::atomic::{AtomicU64, Ordering};

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
        self.offset
            .cmp(&other.offset)
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
        let pos = self
            .anchors
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
    pub fn adjust_offset(offset: usize, bias: AnchorBias, edit: &TextEdit) -> usize {
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
                AnchorBias::Left => offset,        // Stay before inserted text
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
        self.anchors
            .iter()
            .filter(move |a| a.offset >= start && a.offset <= end)
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
        if s <= e {
            (s, e)
        } else {
            (e, s)
        }
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
