//! Line Width Tracker - Viewport-Based Approach
//!
//! Like Monaco/VS Code, we only track the max width of VISIBLE lines.
//! This gives O(visible_lines) performance instead of O(all_lines).
//!
//! The horizontal scrollbar adjusts dynamically as you scroll through
//! the document and encounter longer lines.

/// Tracks line widths using a viewport-based approach (like VS Code/Monaco).
///
/// Key insight: We don't need to know the max width of the ENTIRE document upfront.
/// We only need to track the max width we've SEEN so far in visible lines.
///
/// Operations:
/// - `max_width()`: O(1) - get the maximum width seen so far
/// - `update_visible_range()`: O(visible_lines) - update max from visible lines
/// - No upfront O(n) scan needed!
#[derive(Clone, Debug)]
pub struct LineWidthTracker {
    /// Maximum width seen so far (grows as user scrolls through document)
    cached_max: u32,
    /// Number of lines in the document
    line_count: usize,
    /// Version for change detection
    version: u64,
}

impl Default for LineWidthTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl LineWidthTracker {
    /// Create a new empty tracker
    pub fn new() -> Self {
        Self {
            line_count: 0,
            cached_max: 0,
            version: 0,
        }
    }

    /// Initialize tracker from a rope - O(1), no scanning!
    ///
    /// Unlike the old approach, we don't scan all lines.
    /// Max width will be discovered as the user scrolls.
    pub fn from_rope(rope: &ropey::Rope) -> Self {
        Self {
            line_count: rope.len_lines(),
            cached_max: 0, // Will be updated as visible lines are rendered
            version: 1,
        }
    }

    /// Rebuild the tracker (call when text is entirely replaced)
    pub fn rebuild(&mut self, rope: &ropey::Rope) {
        self.line_count = rope.len_lines();
        // Reset max - it will be rediscovered from visible lines
        self.cached_max = 0;
        self.version += 1;
    }

    /// Get the maximum line width seen so far - O(1)
    pub fn max_width(&self) -> u32 {
        self.cached_max
    }

    /// Update max width from a visible line range
    /// Call this during rendering with the currently visible lines
    pub fn update_visible_range(&mut self, rope: &ropey::Rope, start_line: usize, end_line: usize) {
        let end = end_line.min(rope.len_lines());
        for line_idx in start_line..end {
            let line = rope.line(line_idx);
            let len = line.len_chars();
            let width = if len > 0 && line.char(len - 1) == '\n' {
                (len - 1) as u32
            } else {
                len as u32
            };
            if width > self.cached_max {
                self.cached_max = width;
            }
        }
    }

    /// Update a single line's width after an edit
    pub fn update_line(&mut self, _line_index: usize, new_width: u32) {
        if new_width > self.cached_max {
            self.cached_max = new_width;
        }
        self.version += 1;
    }

    /// Update line width from rope (convenience method)
    pub fn update_line_from_rope(&mut self, rope: &ropey::Rope, line_index: usize) {
        if line_index < rope.len_lines() {
            let line = rope.line(line_index);
            let len = line.len_chars();
            let width = if len > 0 && line.char(len - 1) == '\n' {
                (len - 1) as u32
            } else {
                len as u32
            };
            self.update_line(line_index, width);
        }
    }

    /// Handle line insertion
    pub fn insert_line(&mut self, rope: &ropey::Rope) {
        self.line_count = rope.len_lines();
    }

    /// Handle line deletion
    pub fn delete_line(&mut self, rope: &ropey::Rope) {
        self.line_count = rope.len_lines();
    }

    /// Get the current line count
    pub fn line_count(&self) -> usize {
        self.line_count
    }

    /// Check if the tracker is empty
    pub fn is_empty(&self) -> bool {
        self.line_count == 0
    }

    /// Get the version (incremented on each update)
    pub fn version(&self) -> u64 {
        self.version
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ropey::Rope;

    #[test]
    fn test_empty_rope() {
        let rope = Rope::from_str("");
        let tracker = LineWidthTracker::from_rope(&rope);
        assert_eq!(tracker.max_width(), 0);
    }

    #[test]
    fn test_initial_max_is_zero() {
        // With viewport-based approach, initial max is 0
        let rope = Rope::from_str("hello world");
        let tracker = LineWidthTracker::from_rope(&rope);
        assert_eq!(tracker.max_width(), 0); // Not yet seen any lines
    }

    #[test]
    fn test_update_visible_range() {
        let rope = Rope::from_str("short\nthis is a longer line\nmed");
        let mut tracker = LineWidthTracker::from_rope(&rope);

        // Initially 0
        assert_eq!(tracker.max_width(), 0);

        // After seeing all lines
        tracker.update_visible_range(&rope, 0, 3);
        assert_eq!(tracker.max_width(), 21); // "this is a longer line"
    }

    #[test]
    fn test_incremental_discovery() {
        let rope = Rope::from_str("short\nthis is a longer line\nmed");
        let mut tracker = LineWidthTracker::from_rope(&rope);

        // See only first line
        tracker.update_visible_range(&rope, 0, 1);
        assert_eq!(tracker.max_width(), 5); // "short"

        // Now see the longer line
        tracker.update_visible_range(&rope, 1, 2);
        assert_eq!(tracker.max_width(), 21); // Updated to longer line
    }

    #[test]
    fn test_update_line() {
        let rope = Rope::from_str("short\nlong line here\nmed");
        let mut tracker = LineWidthTracker::from_rope(&rope);
        tracker.update_visible_range(&rope, 0, 3);
        assert_eq!(tracker.max_width(), 14); // "long line here"

        // Typing makes a line longer
        tracker.update_line(1, 20);
        assert_eq!(tracker.max_width(), 20);
    }
}
