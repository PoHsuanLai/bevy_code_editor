//! Display Snapshot - Composes all display map layers
//!
//! The snapshot provides a consistent view of the display map at a point in time,
//! composing all layers to convert between buffer and display coordinates.

use super::{
    BufferPoint, FoldPoint, WrapPoint, DisplayPoint,
    FoldMap, WrapMap, TabMap, DisplayMapLayer,
};

/// A snapshot of the display map state for consistent coordinate conversion
#[derive(Clone, Debug)]
pub struct DisplaySnapshot {
    pub(crate) fold_map: FoldMap,
    pub(crate) wrap_map: WrapMap,
    pub(crate) tab_map: TabMap,
}

impl DisplaySnapshot {
    /// Convert a buffer point to a display point through all layers
    pub fn to_display_point(&self, buffer_point: BufferPoint) -> DisplayPoint {
        let fold_point = self.fold_map.to_output(buffer_point);
        let wrap_point = self.wrap_map.to_output(fold_point);
        self.tab_map.to_output(wrap_point)
    }

    /// Convert a display point back to a buffer point through all layers
    pub fn to_buffer_point(&self, display_point: DisplayPoint) -> BufferPoint {
        let wrap_point = self.tab_map.to_input(display_point);
        let fold_point = self.wrap_map.to_input(wrap_point);
        self.fold_map.to_input(fold_point)
    }

    /// Convert a buffer point to a fold point
    pub fn to_fold_point(&self, buffer_point: BufferPoint) -> FoldPoint {
        self.fold_map.to_output(buffer_point)
    }

    /// Convert a fold point to a wrap point
    pub fn to_wrap_point(&self, fold_point: FoldPoint) -> WrapPoint {
        self.wrap_map.to_output(fold_point)
    }

    /// Get the total number of display rows
    pub fn display_row_count(&self) -> u32 {
        self.wrap_map.output_row_count()
    }

    /// Get the number of visible lines (after folding, before wrapping)
    pub fn visible_line_count(&self) -> u32 {
        self.fold_map.visible_line_count()
    }

    /// Check if a buffer line is hidden (inside a fold)
    pub fn is_buffer_line_hidden(&self, buffer_line: u32) -> bool {
        self.fold_map.is_line_hidden(buffer_line)
    }

    /// Get the buffer row for a display row
    pub fn display_row_to_buffer_row(&self, display_row: u32) -> u32 {
        let display_point = DisplayPoint::new(display_row, 0);
        let buffer_point = self.to_buffer_point(display_point);
        buffer_point.row()
    }

    /// Get the display row for a buffer row
    pub fn buffer_row_to_display_row(&self, buffer_row: u32) -> u32 {
        let buffer_point = BufferPoint::new(buffer_row, 0);
        let display_point = self.to_display_point(buffer_point);
        display_point.row()
    }

    /// Get information about how a buffer row is displayed
    pub fn buffer_row_display_info(&self, buffer_row: u32) -> BufferRowDisplayInfo {
        // First check if the line is hidden
        if self.fold_map.is_line_hidden(buffer_row) {
            return BufferRowDisplayInfo::Hidden;
        }

        // Get the fold row
        let fold_row = self.fold_map.buffer_to_fold_row(buffer_row);

        // Get wrap info
        if let Some((display_row_start, wrap_count)) = self.wrap_map.wrap_info_for_fold_row(fold_row) {
            if wrap_count > 1 {
                BufferRowDisplayInfo::Wrapped {
                    display_row_start,
                    display_row_count: wrap_count,
                }
            } else {
                BufferRowDisplayInfo::Single {
                    display_row: display_row_start,
                }
            }
        } else {
            BufferRowDisplayInfo::Single {
                display_row: fold_row,
            }
        }
    }

    /// Iterate over display rows, yielding buffer row and wrap offset
    pub fn display_rows(&self) -> impl Iterator<Item = DisplayRowInfo> + '_ {
        (0..self.display_row_count()).map(move |display_row| {
            let buffer_point = self.to_buffer_point(DisplayPoint::new(display_row, 0));

            // Determine if this is a wrapped continuation
            let fold_point = FoldPoint::new(
                self.fold_map.buffer_to_fold_row(buffer_point.row()),
                0
            );
            let first_display_row = self.wrap_map.to_output(fold_point).row();
            let is_wrap_continuation = display_row > first_display_row;

            DisplayRowInfo {
                display_row,
                buffer_row: buffer_point.row(),
                is_wrap_continuation,
            }
        })
    }

    /// Get the fold map
    pub fn fold_map(&self) -> &FoldMap {
        &self.fold_map
    }

    /// Get the wrap map
    pub fn wrap_map(&self) -> &WrapMap {
        &self.wrap_map
    }

    /// Get the tab map
    pub fn tab_map(&self) -> &TabMap {
        &self.tab_map
    }
}

/// Information about how a buffer row is displayed
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferRowDisplayInfo {
    /// The row is hidden (inside a fold)
    Hidden,
    /// The row is displayed on a single display row
    Single { display_row: u32 },
    /// The row is wrapped across multiple display rows
    Wrapped {
        display_row_start: u32,
        display_row_count: u32,
    },
}

/// Information about a display row
#[derive(Clone, Copy, Debug)]
pub struct DisplayRowInfo {
    /// The display row index
    pub display_row: u32,
    /// The corresponding buffer row
    pub buffer_row: u32,
    /// Whether this is a wrapped continuation of the previous line
    pub is_wrap_continuation: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FoldRegion, FoldKind};
    use ropey::Rope;

    /// Helper to create a folded region for tests
    fn folded_region(start: usize, end: usize) -> FoldRegion {
        let mut region = FoldRegion::new(start, end, FoldKind::Block);
        region.is_folded = true;
        region
    }

    #[test]
    fn test_simple_conversion() {
        let rope = Rope::from_str("line 1\nline 2\nline 3\n");

        let mut fold_map = FoldMap::new();
        fold_map.update(&rope, &[]);

        let mut wrap_map = WrapMap::new(80);
        wrap_map.update(&rope, &fold_map);

        let tab_map = TabMap::new(4);

        let snapshot = DisplaySnapshot {
            fold_map,
            wrap_map,
            tab_map,
        };

        // Simple case: no folds, no wrapping
        let buffer_point = BufferPoint::new(1, 3);
        let display_point = snapshot.to_display_point(buffer_point);
        assert_eq!(display_point.row(), 1);
        assert_eq!(display_point.column(), 3);

        // Round-trip
        let back = snapshot.to_buffer_point(display_point);
        assert_eq!(back.row(), 1);
        assert_eq!(back.column(), 3);
    }

    #[test]
    fn test_with_fold() {
        let rope = Rope::from_str("line 1\nline 2\nline 3\nline 4\nline 5\n");

        let fold_regions = vec![folded_region(1, 3)]; // Fold lines 2-4

        let mut fold_map = FoldMap::new();
        fold_map.update(&rope, &fold_regions);

        let mut wrap_map = WrapMap::new(80);
        wrap_map.update(&rope, &fold_map);

        let tab_map = TabMap::new(4);

        let snapshot = DisplaySnapshot {
            fold_map,
            wrap_map,
            tab_map,
        };

        // Line 0 -> display row 0
        assert_eq!(snapshot.buffer_row_to_display_row(0), 0);

        // Line 1 (fold start) -> display row 1
        assert_eq!(snapshot.buffer_row_to_display_row(1), 1);

        // Line 4 (after fold) -> display row 2
        assert_eq!(snapshot.buffer_row_to_display_row(4), 2);
    }
}
