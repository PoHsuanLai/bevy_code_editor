//! Tab Map - Expands tabs to spaces for consistent column display
//!
//! The tab map handles tab expansion, converting tab characters to their
//! visual column positions based on tab stops.

use ropey::Rope;
use super::{WrapPoint, DisplayPoint, DisplayMapLayer};

/// Handles tab expansion for display
#[derive(Clone, Debug)]
pub struct TabMap {
    /// Tab size in spaces
    tab_size: u32,
}

impl Default for TabMap {
    fn default() -> Self {
        Self::new(4)
    }
}

impl TabMap {
    /// Create a new tab map with the specified tab size
    pub fn new(tab_size: u32) -> Self {
        Self {
            tab_size: tab_size.max(1),
        }
    }

    /// Update the tab map (currently a no-op since tab expansion is stateless)
    pub fn update(&mut self, _rope: &Rope) {
        // Tab expansion is computed on-demand based on line content
        // No precomputation needed
    }

    /// Set the tab size
    pub fn set_tab_size(&mut self, size: u32) {
        self.tab_size = size.max(1);
    }

    /// Get the tab size
    pub fn tab_size(&self) -> u32 {
        self.tab_size
    }

    /// Expand a column position accounting for tabs in the line
    ///
    /// Given a character column (counting tabs as 1), returns the visual column
    /// (counting tabs as expanded to tab stops).
    pub fn expand_column(&self, line: &str, char_column: u32) -> u32 {
        let mut visual_col = 0u32;
        let mut char_col = 0u32;

        for ch in line.chars() {
            if char_col >= char_column {
                break;
            }

            if ch == '\t' {
                // Advance to next tab stop
                let next_stop = ((visual_col / self.tab_size) + 1) * self.tab_size;
                visual_col = next_stop;
            } else if ch == '\n' {
                break;
            } else {
                visual_col += 1;
            }

            char_col += 1;
        }

        visual_col
    }

    /// Contract a visual column to a character column
    ///
    /// Given a visual column (with tabs expanded), returns the character column
    /// (counting tabs as 1).
    pub fn contract_column(&self, line: &str, visual_column: u32) -> u32 {
        let mut visual_col = 0u32;
        let mut char_col = 0u32;

        for ch in line.chars() {
            if visual_col >= visual_column {
                break;
            }

            let char_width = if ch == '\t' {
                // Tab width depends on current visual column
                let next_stop = ((visual_col / self.tab_size) + 1) * self.tab_size;
                next_stop - visual_col
            } else if ch == '\n' {
                break;
            } else {
                1
            };

            // Check if we'd overshoot
            if visual_col + char_width > visual_column {
                // Stop at this character
                break;
            }

            visual_col += char_width;
            char_col += 1;
        }

        char_col
    }

    /// Get the visual width of a line (with tabs expanded)
    pub fn line_visual_width(&self, line: &str) -> u32 {
        let mut visual_col = 0u32;

        for ch in line.chars() {
            if ch == '\t' {
                let next_stop = ((visual_col / self.tab_size) + 1) * self.tab_size;
                visual_col = next_stop;
            } else if ch == '\n' {
                break;
            } else {
                visual_col += 1;
            }
        }

        visual_col
    }
}

impl DisplayMapLayer for TabMap {
    type InputPoint = WrapPoint;
    type OutputPoint = DisplayPoint;

    fn to_output(&self, point: WrapPoint) -> DisplayPoint {
        // Tab expansion is context-dependent (needs line content)
        // For now, we assume columns are already visual columns at this stage
        // The actual tab expansion happens during rendering when we have line content
        DisplayPoint::new(point.row(), point.column())
    }

    fn to_input(&self, point: DisplayPoint) -> WrapPoint {
        // Same as above - actual contraction happens with line content
        WrapPoint::new(point.row(), point.column())
    }

    fn output_row_count(&self) -> u32 {
        // Tab map doesn't change row count
        u32::MAX // Indicates "same as input"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_tabs() {
        let tab_map = TabMap::new(4);
        let line = "hello world";

        assert_eq!(tab_map.expand_column(line, 0), 0);
        assert_eq!(tab_map.expand_column(line, 5), 5);
        assert_eq!(tab_map.expand_column(line, 11), 11);
    }

    #[test]
    fn test_single_tab() {
        let tab_map = TabMap::new(4);
        let line = "\thello";

        // Tab at position 0 expands to visual column 0-3
        assert_eq!(tab_map.expand_column(line, 0), 0);
        assert_eq!(tab_map.expand_column(line, 1), 4); // After tab
        assert_eq!(tab_map.expand_column(line, 2), 5); // 'h'
    }

    #[test]
    fn test_multiple_tabs() {
        let tab_map = TabMap::new(4);
        let line = "a\tb\tc";

        // 'a' at visual 0, tab at 1-3, 'b' at visual 4, tab at 5-7, 'c' at visual 8
        assert_eq!(tab_map.expand_column(line, 0), 0); // 'a'
        assert_eq!(tab_map.expand_column(line, 1), 1); // position of tab
        assert_eq!(tab_map.expand_column(line, 2), 4); // 'b'
        assert_eq!(tab_map.expand_column(line, 3), 5); // position of second tab
        assert_eq!(tab_map.expand_column(line, 4), 8); // 'c'
    }

    #[test]
    fn test_contract_column() {
        let tab_map = TabMap::new(4);
        let line = "\thello";

        assert_eq!(tab_map.contract_column(line, 0), 0);
        assert_eq!(tab_map.contract_column(line, 4), 1); // After tab
        assert_eq!(tab_map.contract_column(line, 5), 2); // 'h'
    }

    #[test]
    fn test_line_visual_width() {
        let tab_map = TabMap::new(4);

        assert_eq!(tab_map.line_visual_width("hello"), 5);
        assert_eq!(tab_map.line_visual_width("\thello"), 9); // 4 + 5
        assert_eq!(tab_map.line_visual_width("a\tb"), 5); // 1 + 3 + 1
    }
}
