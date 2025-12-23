//! Wrap Map - Transforms fold coordinates by wrapping long lines
//!
//! The wrap map handles soft line wrapping, where a single buffer/fold line
//! can be displayed across multiple screen lines.

use ropey::Rope;
use super::{FoldPoint, WrapPoint, FoldMap, DisplayMapLayer};

/// Information about how a single fold line is wrapped
#[derive(Clone, Debug)]
struct WrapInfo {
    /// The fold line index
    fold_row: u32,
    /// Number of display rows this fold line takes
    wrap_count: u32,
    /// Starting display row for this fold line
    display_row_start: u32,
}

/// Tracks line wrapping for soft wrap display
#[derive(Clone, Debug)]
pub struct WrapMap {
    /// Wrap width in characters
    wrap_width: u32,
    /// Wrap info for each fold line
    wrap_info: Vec<WrapInfo>,
    /// Total number of display rows after wrapping
    total_display_rows: u32,
    /// Whether wrapping is enabled
    enabled: bool,
}

impl Default for WrapMap {
    fn default() -> Self {
        Self::new(80)
    }
}

impl WrapMap {
    /// Create a new wrap map with the specified wrap width
    pub fn new(wrap_width: u32) -> Self {
        Self {
            wrap_width: wrap_width.max(1),
            wrap_info: Vec::new(),
            total_display_rows: 0,
            enabled: true,
        }
    }

    /// Update the wrap map from buffer content and fold map
    pub fn update(&mut self, rope: &Rope, fold_map: &FoldMap) {
        self.wrap_info.clear();

        if !self.enabled || self.wrap_width == 0 {
            // No wrapping - each fold line is one display line
            let count = fold_map.visible_line_count();
            for i in 0..count {
                self.wrap_info.push(WrapInfo {
                    fold_row: i,
                    wrap_count: 1,
                    display_row_start: i,
                });
            }
            self.total_display_rows = count;
            return;
        }

        let mut display_row = 0u32;

        // Iterate through visible lines (after folding)
        for fold_row in 0..fold_map.visible_line_count() {
            // Get the buffer row for this fold row
            let buffer_row = fold_map.fold_to_buffer_row(fold_row);

            // Get line length
            let line_len = if (buffer_row as usize) < rope.len_lines() {
                let line = rope.line(buffer_row as usize);
                // Exclude trailing newline from length calculation
                let len = line.len_chars();
                if len > 0 && line.char(len - 1) == '\n' {
                    (len - 1) as u32
                } else {
                    len as u32
                }
            } else {
                0
            };

            // Calculate how many display rows this line needs
            let wrap_count = if line_len == 0 {
                1 // Empty lines still take one row
            } else {
                line_len.div_ceil(self.wrap_width).max(1)
            };

            self.wrap_info.push(WrapInfo {
                fold_row,
                wrap_count,
                display_row_start: display_row,
            });

            display_row += wrap_count;
        }

        self.total_display_rows = display_row;
    }

    /// Set the wrap width
    pub fn set_wrap_width(&mut self, width: u32) {
        self.wrap_width = width.max(1);
    }

    /// Enable or disable wrapping
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Get the wrap width
    pub fn wrap_width(&self) -> u32 {
        self.wrap_width
    }

    /// Check if wrapping is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get the total number of display rows
    pub fn display_row_count(&self) -> u32 {
        self.total_display_rows
    }

    /// Convert a fold point to a wrap point
    pub fn fold_to_wrap(&self, point: FoldPoint) -> WrapPoint {
        let fold_row = point.row();
        let column = point.column();

        if let Some(info) = self.wrap_info.get(fold_row as usize) {
            if !self.enabled || self.wrap_width == 0 {
                // No wrapping
                return WrapPoint::new(info.display_row_start, column);
            }

            // Calculate which wrapped line and column
            let wrap_line = column / self.wrap_width;
            let wrap_column = column % self.wrap_width;

            let display_row = info.display_row_start + wrap_line.min(info.wrap_count - 1);
            WrapPoint::new(display_row, wrap_column)
        } else {
            // Beyond end of document
            WrapPoint::new(self.total_display_rows, column)
        }
    }

    /// Convert a wrap point to a fold point
    pub fn wrap_to_fold(&self, point: WrapPoint) -> FoldPoint {
        let display_row = point.row();
        let column = point.column();

        // Binary search for the fold row containing this display row
        let fold_row = match self.wrap_info.binary_search_by(|info| {
            if display_row < info.display_row_start {
                std::cmp::Ordering::Greater
            } else if display_row >= info.display_row_start + info.wrap_count {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        }) {
            Ok(idx) | Err(idx) => {
                if idx < self.wrap_info.len() {
                    idx
                } else if !self.wrap_info.is_empty() {
                    self.wrap_info.len() - 1
                } else {
                    return FoldPoint::ZERO;
                }
            }
        };

        let info = &self.wrap_info[fold_row];
        let wrap_offset = display_row.saturating_sub(info.display_row_start);

        // Calculate the actual column in the fold line
        let fold_column = if self.enabled && self.wrap_width > 0 {
            wrap_offset * self.wrap_width + column
        } else {
            column
        };

        FoldPoint::new(info.fold_row, fold_column)
    }

    /// Get wrap information for a fold row
    pub fn wrap_info_for_fold_row(&self, fold_row: u32) -> Option<(u32, u32)> {
        self.wrap_info.get(fold_row as usize).map(|info| {
            (info.display_row_start, info.wrap_count)
        })
    }
}

impl DisplayMapLayer for WrapMap {
    type InputPoint = FoldPoint;
    type OutputPoint = WrapPoint;

    fn to_output(&self, point: FoldPoint) -> WrapPoint {
        self.fold_to_wrap(point)
    }

    fn to_input(&self, point: WrapPoint) -> FoldPoint {
        self.wrap_to_fold(point)
    }

    fn output_row_count(&self) -> u32 {
        self.total_display_rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_wrap() {
        let rope = Rope::from_str("short\n");
        let mut fold_map = FoldMap::new();
        fold_map.update(&rope, &[]);

        let mut wrap_map = WrapMap::new(80);
        wrap_map.update(&rope, &fold_map);

        // "short\n" has 2 lines in ropey (the text line and trailing empty)
        assert_eq!(wrap_map.display_row_count(), 2);

        let fold_point = FoldPoint::new(0, 3);
        let wrap_point = wrap_map.fold_to_wrap(fold_point);
        assert_eq!(wrap_point.row(), 0);
        assert_eq!(wrap_point.column(), 3);
    }

    #[test]
    fn test_simple_wrap() {
        let rope = Rope::from_str("0123456789\n"); // 10 chars + newline
        let mut fold_map = FoldMap::new();
        fold_map.update(&rope, &[]);

        let mut wrap_map = WrapMap::new(4); // Wrap at 4 chars
        wrap_map.update(&rope, &fold_map);

        // Line 0: "0123456789" (10 chars) wraps to 3 display lines
        // Line 1: "" (empty after newline) = 1 display line
        // Total: 4 display rows
        assert_eq!(wrap_map.display_row_count(), 4);

        // Column 0 -> row 0, col 0
        let p = wrap_map.fold_to_wrap(FoldPoint::new(0, 0));
        assert_eq!((p.row(), p.column()), (0, 0));

        // Column 5 -> row 1, col 1
        let p = wrap_map.fold_to_wrap(FoldPoint::new(0, 5));
        assert_eq!((p.row(), p.column()), (1, 1));

        // Column 9 -> row 2, col 1
        let p = wrap_map.fold_to_wrap(FoldPoint::new(0, 9));
        assert_eq!((p.row(), p.column()), (2, 1));
    }
}
