//! Display Map - Composable coordinate transformation layers
//!
//! The display map transforms buffer coordinates to screen coordinates through
//! a series of composable layers. Each layer handles a specific transformation:
//!
//! - **FoldMap**: Hides folded regions, collapsing multiple buffer lines into one
//! - **WrapMap**: Wraps long lines visually without modifying the buffer
//! - **TabMap**: Expands tabs to spaces for consistent display width
//!
//! ## Coordinate Systems
//!
//! - **BufferPoint**: Position in the raw text buffer (line, column in chars)
//! - **FoldPoint**: Position after applying folds (some lines hidden)
//! - **WrapPoint**: Position after applying soft wraps (one buffer line → multiple display lines)
//! - **DisplayPoint**: Final screen position (after all transforms)
//!
//! ## Usage
//!
//! ```ignore
//! let snapshot = display_map.snapshot();
//!
//! // Convert buffer position to screen position
//! let display_point = snapshot.to_display_point(buffer_point);
//!
//! // Convert screen click to buffer position
//! let buffer_point = snapshot.to_buffer_point(display_point);
//! ```

mod point;
mod fold_map;
mod wrap_map;
mod tab_map;
mod snapshot;

pub use point::*;
pub use fold_map::*;
pub use wrap_map::*;
pub use tab_map::*;
pub use snapshot::*;

use bevy::prelude::*;
use ropey::Rope;

/// A row/column point in a coordinate space
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Point {
    pub row: u32,
    pub column: u32,
}

impl Point {
    pub const ZERO: Point = Point { row: 0, column: 0 };

    pub fn new(row: u32, column: u32) -> Self {
        Self { row, column }
    }
}

/// Trait for display map layers that transform coordinates
pub trait DisplayMapLayer {
    /// The input point type for this layer
    type InputPoint: Copy;
    /// The output point type for this layer
    type OutputPoint: Copy;

    /// Transform a point from input to output coordinate space
    fn to_output(&self, point: Self::InputPoint) -> Self::OutputPoint;

    /// Transform a point from output to input coordinate space
    fn to_input(&self, point: Self::OutputPoint) -> Self::InputPoint;

    /// Get the number of output rows
    fn output_row_count(&self) -> u32;
}

/// Layered display map with composable coordinate transformations
///
/// This is a more advanced display map that composes multiple transformation
/// layers (fold, wrap, tab). It's separate from the simpler `DisplayMap` in
/// `types.rs` which handles basic soft wrapping.
///
/// Use this for advanced coordinate transformations that need to compose
/// multiple layers together.
#[derive(Resource, Clone, Debug)]
pub struct LayeredDisplayMap {
    /// The fold map layer
    pub fold_map: FoldMap,
    /// The wrap map layer
    pub wrap_map: WrapMap,
    /// The tab map layer
    pub tab_map: TabMap,
    /// Version counter for change detection
    pub version: u64,
}

impl Default for LayeredDisplayMap {
    fn default() -> Self {
        Self {
            fold_map: FoldMap::new(),
            wrap_map: WrapMap::new(80), // Default wrap width
            tab_map: TabMap::new(4),    // Default tab size
            version: 0,
        }
    }
}

impl LayeredDisplayMap {
    /// Create a new layered display map with specified settings
    pub fn new(wrap_width: u32, tab_size: u32) -> Self {
        Self {
            fold_map: FoldMap::new(),
            wrap_map: WrapMap::new(wrap_width),
            tab_map: TabMap::new(tab_size),
            version: 0,
        }
    }

    /// Update the display map from buffer content and fold regions
    pub fn update(&mut self, rope: &Rope, fold_regions: &[FoldRegion]) {
        self.fold_map.update(rope, fold_regions);
        self.wrap_map.update(rope, &self.fold_map);
        self.tab_map.update(rope);
        self.version += 1;
    }

    /// Update the display map from buffer content and FoldState resource
    /// This is the preferred method when using the FoldState resource from types.rs
    pub fn update_from_fold_state(&mut self, rope: &Rope, fold_state: &crate::types::FoldState) {
        self.fold_map.update(rope, &fold_state.regions);
        self.wrap_map.update(rope, &self.fold_map);
        self.tab_map.update(rope);
        self.version += 1;
    }

    /// Create a snapshot of the current display map state
    pub fn snapshot(&self) -> DisplaySnapshot {
        DisplaySnapshot {
            fold_map: self.fold_map.clone(),
            wrap_map: self.wrap_map.clone(),
            tab_map: self.tab_map.clone(),
        }
    }

    /// Set the wrap width (in characters)
    pub fn set_wrap_width(&mut self, width: u32) {
        self.wrap_map.set_wrap_width(width);
    }

    /// Set the tab size (in spaces)
    pub fn set_tab_size(&mut self, size: u32) {
        self.tab_map.set_tab_size(size);
    }
}

// Re-export fold types from types.rs to avoid duplication
pub use crate::types::{FoldRegion, FoldState, FoldKind};
