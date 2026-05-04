//! `DisplayLayout` — the immutable, paint-ready snapshot the renderer consumes.
//!
//! The Warp insight: a frame's render is a pure function of its layout. We get
//! cheap "nothing changed" by comparing `Arc::ptr_eq(&prev.lines, &next.lines)`
//! and skipping the GPU upload entirely. Scroll-only updates bump
//! `scroll_version` but reuse the same `lines` Arc.

use bevy::prelude::*;
use std::ops::Range;
use std::sync::Arc;

use super::snapshot::ShapedLine;

/// Per-entity rendering snapshot. Replaces the dirty-flag dance on `TextViewState`.
///
/// Written by `display_map::build_display_layout` (or by `trivial_layout` for
/// standalone consumers). Read-only for the renderer.
#[derive(Component, Clone)]
pub struct DisplayLayout {
    /// Visible-window slice of shaped lines. Shared (Arc) so scroll-only and
    /// content-only paths can swap one without rebuilding the other.
    pub lines: Arc<Vec<ShapedLine>>,
    /// Display row range covered by `lines` (absolute, into the full document).
    pub visible_rows: Range<u32>,
    /// Total display row count for the entire document (for scrollbar sizing).
    pub total_display_rows: u32,
    pub line_height: f32,
    /// Width of one column in pixels. Monospace assumption — for proportional fonts
    /// this is a hint only and per-glyph advance from shaping wins.
    pub char_width: f32,
    /// Vertical baseline offset within a line, in pixels.
    pub baseline_offset: f32,
    /// Default foreground color when a `ShapedLine.runs` is empty.
    pub default_fg: Color,
    /// Bumps when content / wrap / fold / styling changes (anything that invalidates `lines`).
    pub version: u64,
    /// Bumps independently when only scroll changed — same `lines`, different viewport slice.
    pub scroll_version: u64,
}

impl Default for DisplayLayout {
    fn default() -> Self {
        Self {
            lines: Arc::new(Vec::new()),
            visible_rows: 0..0,
            total_display_rows: 0,
            line_height: 16.0,
            char_width: 8.0,
            baseline_offset: 0.0,
            default_fg: Color::WHITE,
            version: 0,
            scroll_version: 0,
        }
    }
}

impl DisplayLayout {
    /// True when the same `lines` Arc backs both layouts (cheap nothing-changed check).
    pub fn lines_unchanged(&self, other: &DisplayLayout) -> bool {
        Arc::ptr_eq(&self.lines, &other.lines)
    }
}
