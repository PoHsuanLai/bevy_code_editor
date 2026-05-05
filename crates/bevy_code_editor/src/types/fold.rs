//! Code folding types

use bevy::prelude::*;

use super::editor::{CursorState, SelectionState};
use crate::text_view::TextViewState;

/// Per-editor "go to line" dialog state.
#[derive(Clone, Debug, Default, Component, Reflect)]
#[reflect(Component, Default, Debug)]
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
    pub fn goto(
        &self,
        sel: &mut SelectionState,
        cursor: &mut CursorState,
        tv: &mut TextViewState,
    ) -> bool {
        if let Some(line_num) = self.parse_line_number() {
            let total_lines = tv.rope.len_lines();
            // Clamp line number to valid range (1-indexed input, convert to 0-indexed)
            let target_line = line_num
                .saturating_sub(1)
                .min(total_lines.saturating_sub(1));

            // Move cursor to the start of the target line
            let char_pos = tv.rope.line_to_char(target_line);
            cursor.cursor_pos = char_pos;
            sel.apply_primary_cursor(cursor);

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
#[derive(Clone, Debug, PartialEq, Eq, Reflect)]
#[reflect(Debug, PartialEq)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
#[reflect(Debug, PartialEq, Hash)]
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
            FoldKind::Function => '\u{0192}',
            FoldKind::Class => '\u{25C6}',
            FoldKind::Comment => '/',
            _ => '\u{25B6}',
        }
    }
}

/// Per-editor fold-region state.
#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component, Default, Debug)]
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
        let pos = self
            .regions
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
        self.regions
            .iter()
            .any(|r| r.start_line == line && r.is_folded)
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
        self.regions
            .iter()
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
        self.regions
            .iter()
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
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct FoldIndicator {
    /// The line this indicator is for
    pub line_index: usize,
}
