//! TextViewState — generic text view state component
//!
//! Holds the text buffer, scroll offsets, dirty tracking, and pre-computed styled lines.
//! This is the reusable core that both the code editor and other text views (chat, logs) share.

use bevy::prelude::*;
use ropey::Rope;

use super::line_width::LineWidthTracker;

/// Generic text view state — holds the rope buffer and scroll state for a
/// scrollable text-rendering entity.
///
/// As of step 11 the dirty-tracking flags (`needs_update`, `dirty_lines`, etc.)
/// and the legacy per-line styling cache (`styled_lines`) are gone. Rendering
/// invalidation now flows through `display_map::LayoutFingerprint` (which
/// reads `content_version` and scroll/viewport changes) and Bevy's
/// `Changed<DisplayLayout>` / `Changed<TextViewOverlays>` change detection.
#[derive(Component)]
pub struct TextViewState {
    /// The text content (efficient rope data structure)
    pub rope: Rope,

    /// Current vertical scroll offset in pixels
    pub scroll_offset: f32,
    /// Target vertical scroll offset for smooth scrolling
    pub target_scroll_offset: f32,
    /// Current horizontal scroll offset in pixels
    pub horizontal_scroll_offset: f32,
    /// Target horizontal scroll offset for smooth scrolling
    pub target_horizontal_scroll_offset: f32,

    /// Monotonic counter bumped on every rope mutation. Read by the display
    /// map's fingerprint to decide whether to rebuild the layout.
    pub content_version: u64,

    /// Maximum content width (longest line in pixels)
    pub max_content_width: f32,
    /// The line index that has the max width
    pub max_width_line: Option<usize>,
    /// Line width tracker for O(log n) max line width queries
    pub line_width_tracker: LineWidthTracker,
}

impl Default for TextViewState {
    fn default() -> Self {
        Self {
            rope: Rope::from_str(""),
            scroll_offset: 0.0,
            target_scroll_offset: 0.0,
            horizontal_scroll_offset: 0.0,
            target_horizontal_scroll_offset: 0.0,
            content_version: 0,
            max_content_width: 0.0,
            max_width_line: None,
            line_width_tracker: LineWidthTracker::new(),
        }
    }
}

impl TextViewState {
    /// Create a new text view with initial text content
    pub fn with_text(text: &str) -> Self {
        let rope = Rope::from_str(text);
        let line_width_tracker = LineWidthTracker::from_rope(&rope);
        Self {
            rope,
            line_width_tracker,
            content_version: 1,
            ..Default::default()
        }
    }

    /// Get total number of lines in the buffer
    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    /// Get the full text as a String
    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    /// Replace the entire text content
    pub fn set_text(&mut self, text: &str) {
        self.rope = Rope::from_str(text);
        self.content_version += 1;
        self.line_width_tracker = LineWidthTracker::from_rope(&self.rope);
    }

    /// Bump `content_version` to force a layout rebuild on the next frame.
    /// Use after any rope mutation that didn't go through `set_text`.
    pub fn bump_version(&mut self) {
        self.content_version += 1;
    }
}
