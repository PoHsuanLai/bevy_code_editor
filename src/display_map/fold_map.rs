//! Fold Map - Transforms buffer coordinates by hiding folded regions
//!
//! The fold map handles code folding by mapping buffer lines to fold lines,
//! where folded regions are collapsed to a single line.

use ropey::Rope;
use super::{BufferPoint, FoldPoint, FoldRegion, DisplayMapLayer};

/// Cached summary for a fold region to enable O(log n) lookups
#[derive(Clone, Debug)]
struct FoldSummary {
    /// The fold region
    region: FoldRegion,
    /// Cumulative hidden lines before this fold (exclusive)
    hidden_before: u32,
    /// Cumulative hidden lines including this fold (inclusive)
    hidden_through: u32,
}

/// Tracks which buffer lines map to which fold lines
#[derive(Clone, Debug)]
pub struct FoldMap {
    /// Sorted list of folded regions with cached summaries for O(log n) lookup
    fold_summaries: Vec<FoldSummary>,
    /// Total number of visible lines after folding
    visible_line_count: u32,
    /// Total number of buffer lines
    buffer_line_count: u32,
}

impl Default for FoldMap {
    fn default() -> Self {
        Self::new()
    }
}

impl FoldMap {
    /// Create a new empty fold map
    pub fn new() -> Self {
        Self {
            fold_summaries: Vec::new(),
            visible_line_count: 0,
            buffer_line_count: 0,
        }
    }

    /// Update the fold map from buffer content and fold regions
    pub fn update(&mut self, rope: &Rope, fold_regions: &[FoldRegion]) {
        self.buffer_line_count = rope.len_lines() as u32;

        // Filter and sort folded regions
        let mut folded: Vec<_> = fold_regions
            .iter()
            .filter(|r| r.is_folded)
            .cloned()
            .collect();
        folded.sort_by_key(|r| r.start_line);

        // Build summaries with cumulative hidden line counts for O(log n) lookup
        self.fold_summaries.clear();
        let mut cumulative_hidden = 0u32;

        for region in folded {
            let hidden = region.hidden_line_count() as u32;
            self.fold_summaries.push(FoldSummary {
                region,
                hidden_before: cumulative_hidden,
                hidden_through: cumulative_hidden + hidden,
            });
            cumulative_hidden += hidden;
        }

        self.visible_line_count = self.buffer_line_count.saturating_sub(cumulative_hidden);
    }

    /// Check if a buffer line is hidden (inside a fold) - O(log n)
    pub fn is_line_hidden(&self, buffer_line: u32) -> bool {
        let buffer_line = buffer_line as usize;

        // Binary search for a fold that might contain this line
        let idx = self.fold_summaries.partition_point(|s| s.region.start_line < buffer_line);

        // Check if this line is inside the fold at idx-1 (the one just before or at this line)
        if idx > 0 {
            let summary = &self.fold_summaries[idx - 1];
            if buffer_line > summary.region.start_line && buffer_line <= summary.region.end_line {
                return true;
            }
        }

        // Also check the fold at idx (might start before this line)
        if idx < self.fold_summaries.len() {
            let summary = &self.fold_summaries[idx];
            if buffer_line > summary.region.start_line && buffer_line <= summary.region.end_line {
                return true;
            }
        }

        false
    }

    /// Get the fold region that starts at a given line (if any) - O(log n)
    pub fn fold_at_line(&self, buffer_line: u32) -> Option<&FoldRegion> {
        let buffer_line = buffer_line as usize;

        // Binary search for exact match
        self.fold_summaries
            .binary_search_by_key(&buffer_line, |s| s.region.start_line)
            .ok()
            .map(|idx| &self.fold_summaries[idx].region)
    }

    /// Convert a buffer line to a fold line (display line after folding) - O(log n)
    pub fn buffer_to_fold_row(&self, buffer_row: u32) -> u32 {
        let buffer_row_usize = buffer_row as usize;

        if self.fold_summaries.is_empty() {
            return buffer_row;
        }

        // Find the first fold that starts at or after buffer_row
        let idx = self.fold_summaries.partition_point(|s| s.region.start_line < buffer_row_usize);

        // Check if we're inside the previous fold
        if idx > 0 {
            let prev = &self.fold_summaries[idx - 1];
            if buffer_row_usize > prev.region.start_line && buffer_row_usize <= prev.region.end_line {
                // Inside a fold - map to the fold start line
                return (prev.region.start_line as u32).saturating_sub(prev.hidden_before);
            }
            // After the previous fold - use its cumulative hidden count
            return buffer_row.saturating_sub(prev.hidden_through);
        }

        // Before all folds
        buffer_row
    }

    /// Convert a fold line to a buffer line - O(log n)
    pub fn fold_to_buffer_row(&self, fold_row: u32) -> u32 {
        if self.fold_summaries.is_empty() {
            return fold_row;
        }

        // Binary search: find the fold whose fold_start matches or contains fold_row
        // fold_start for a fold = region.start_line - hidden_before
        let idx = self.fold_summaries.partition_point(|s| {
            let fold_start = (s.region.start_line as u32).saturating_sub(s.hidden_before);
            fold_start <= fold_row
        });

        if idx == 0 {
            // Before all folds
            return fold_row;
        }

        let summary = &self.fold_summaries[idx - 1];
        let fold_start = (summary.region.start_line as u32).saturating_sub(summary.hidden_before);

        if fold_row == fold_start {
            // Exactly on a fold line
            return summary.region.start_line as u32;
        }

        // After this fold - add back the hidden lines
        fold_row + summary.hidden_through
    }

    /// Get the number of visible lines (after folding)
    pub fn visible_line_count(&self) -> u32 {
        self.visible_line_count
    }

    /// Get all folded regions
    pub fn folded_regions(&self) -> impl Iterator<Item = &FoldRegion> {
        self.fold_summaries.iter().map(|s| &s.region)
    }

    /// Get number of folded regions
    pub fn fold_count(&self) -> usize {
        self.fold_summaries.len()
    }

    /// Iterator over visible buffer lines (skipping hidden ones)
    pub fn visible_lines(&self) -> impl Iterator<Item = u32> + '_ {
        (0..self.buffer_line_count).filter(|&line| !self.is_line_hidden(line))
    }
}

impl DisplayMapLayer for FoldMap {
    type InputPoint = BufferPoint;
    type OutputPoint = FoldPoint;

    fn to_output(&self, point: BufferPoint) -> FoldPoint {
        let fold_row = self.buffer_to_fold_row(point.row());
        // Column stays the same (folding doesn't affect horizontal position)
        FoldPoint::new(fold_row, point.column())
    }

    fn to_input(&self, point: FoldPoint) -> BufferPoint {
        let buffer_row = self.fold_to_buffer_row(point.row());
        BufferPoint::new(buffer_row, point.column())
    }

    fn output_row_count(&self) -> u32 {
        self.visible_line_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FoldKind;

    /// Helper to create a folded region for tests
    fn folded_region(start: usize, end: usize) -> FoldRegion {
        let mut region = FoldRegion::new(start, end, FoldKind::Block);
        region.is_folded = true;
        region
    }

    #[test]
    fn test_no_folds() {
        let rope = Rope::from_str("line 1\nline 2\nline 3\nline 4\n");
        let mut fold_map = FoldMap::new();
        fold_map.update(&rope, &[]);

        assert_eq!(fold_map.visible_line_count(), 5); // 4 lines + trailing newline
        assert_eq!(fold_map.buffer_to_fold_row(0), 0);
        assert_eq!(fold_map.buffer_to_fold_row(2), 2);
        assert_eq!(fold_map.fold_to_buffer_row(2), 2);
    }

    #[test]
    fn test_single_fold() {
        let rope = Rope::from_str("line 1\nline 2\nline 3\nline 4\nline 5\n");
        let mut fold_map = FoldMap::new();
        let regions = vec![folded_region(1, 3)]; // Fold lines 2-4 (0-indexed: 1-3)
        fold_map.update(&rope, &regions);

        // Lines 2, 3 are hidden (indices 2, 3)
        assert_eq!(fold_map.visible_line_count(), 4); // 6 - 2 hidden = 4
        assert!(!fold_map.is_line_hidden(0));
        assert!(!fold_map.is_line_hidden(1)); // Start line is visible
        assert!(fold_map.is_line_hidden(2));
        assert!(fold_map.is_line_hidden(3));
        assert!(!fold_map.is_line_hidden(4));

        // Row conversion
        assert_eq!(fold_map.buffer_to_fold_row(0), 0);
        assert_eq!(fold_map.buffer_to_fold_row(1), 1);
        assert_eq!(fold_map.buffer_to_fold_row(4), 2);
    }

    #[test]
    fn test_multiple_folds() {
        // 10 lines: 0-9
        let rope = Rope::from_str("0\n1\n2\n3\n4\n5\n6\n7\n8\n9\n");
        let mut fold_map = FoldMap::new();

        // Fold lines 1-2 (hides 2) and lines 5-7 (hides 6, 7)
        let regions = vec![
            folded_region(1, 2), // hides line 2
            folded_region(5, 7), // hides lines 6, 7
        ];
        fold_map.update(&rope, &regions);

        // 11 lines - 1 - 2 = 8 visible
        assert_eq!(fold_map.visible_line_count(), 8);

        // Hidden checks
        assert!(!fold_map.is_line_hidden(0));
        assert!(!fold_map.is_line_hidden(1)); // fold start visible
        assert!(fold_map.is_line_hidden(2));
        assert!(!fold_map.is_line_hidden(3));
        assert!(!fold_map.is_line_hidden(4));
        assert!(!fold_map.is_line_hidden(5)); // fold start visible
        assert!(fold_map.is_line_hidden(6));
        assert!(fold_map.is_line_hidden(7));
        assert!(!fold_map.is_line_hidden(8));

        // Buffer to fold row
        assert_eq!(fold_map.buffer_to_fold_row(0), 0);
        assert_eq!(fold_map.buffer_to_fold_row(1), 1);
        assert_eq!(fold_map.buffer_to_fold_row(2), 1); // inside fold -> fold start
        assert_eq!(fold_map.buffer_to_fold_row(3), 2); // after first fold
        assert_eq!(fold_map.buffer_to_fold_row(4), 3);
        assert_eq!(fold_map.buffer_to_fold_row(5), 4);
        assert_eq!(fold_map.buffer_to_fold_row(8), 5); // after second fold

        // Fold to buffer row
        assert_eq!(fold_map.fold_to_buffer_row(0), 0);
        assert_eq!(fold_map.fold_to_buffer_row(1), 1);
        assert_eq!(fold_map.fold_to_buffer_row(2), 3);
        assert_eq!(fold_map.fold_to_buffer_row(4), 5);
        assert_eq!(fold_map.fold_to_buffer_row(5), 8);
    }

    #[test]
    fn test_fold_at_line() {
        let rope = Rope::from_str("0\n1\n2\n3\n4\n");
        let mut fold_map = FoldMap::new();
        let regions = vec![folded_region(1, 3)];
        fold_map.update(&rope, &regions);

        assert!(fold_map.fold_at_line(0).is_none());
        assert!(fold_map.fold_at_line(1).is_some());
        assert!(fold_map.fold_at_line(2).is_none());
        assert!(fold_map.fold_at_line(4).is_none());
    }
}
