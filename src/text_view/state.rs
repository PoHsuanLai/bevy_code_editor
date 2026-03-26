//! TextViewState — generic text view state component
//!
//! Holds the text buffer, scroll offsets, dirty tracking, and pre-computed styled lines.
//! This is the reusable core that both the code editor and other text views (chat, logs) share.

use bevy::prelude::*;
use ropey::Rope;
use std::ops::Range;

use crate::line_width::LineWidthTracker;
use crate::types::LineSegment;

/// Generic text view state — holds everything needed to render styled text in a scrollable viewport.
///
/// This is a Bevy Component (not a Resource) so multiple text views can coexist as separate entities.
#[derive(Component)]
pub struct TextViewState {
    // === Text Buffer ===
    /// The text content (efficient rope data structure)
    pub rope: Rope,

    // === Scroll ===
    /// Current vertical scroll offset in pixels
    pub scroll_offset: f32,
    /// Target vertical scroll offset for smooth scrolling
    pub target_scroll_offset: f32,
    /// Current horizontal scroll offset in pixels
    pub horizontal_scroll_offset: f32,
    /// Target horizontal scroll offset for smooth scrolling
    pub target_horizontal_scroll_offset: f32,

    // === Update Tracking ===
    /// Needs full re-render (content or style changed)
    pub needs_update: bool,
    /// Only scroll changed — skip glyph rebuilding, just reposition
    pub needs_scroll_update: bool,
    /// Debouncing: update is pending but not yet applied
    pub pending_update: bool,
    /// Last time we rendered (in ms) for debouncing
    pub last_render_time: f64,
    /// Content version — incremented on every text change
    pub content_version: u64,
    /// Which lines need re-rendering (None = all)
    pub dirty_lines: Option<Range<usize>>,
    /// Track line count for detecting changes
    pub previous_line_count: usize,

    // === Layout ===
    /// Maximum content width (longest line in pixels)
    pub max_content_width: f32,
    /// Version when max_content_width was last calculated
    pub max_content_width_version: u64,
    /// The line index that has the max width
    pub max_width_line: Option<usize>,
    /// Line width tracker for O(log n) max line width queries
    pub line_width_tracker: LineWidthTracker,

    // === Styled Content ===
    /// Pre-computed styled line segments for rendering.
    /// Each line is Option: None means "use default foreground color for raw text".
    /// Populated by syntax highlighting, chat styling, or any other content producer.
    pub styled_lines: Vec<Option<Vec<LineSegment>>>,
    /// Version counter for styled lines (for cache invalidation)
    pub styled_lines_version: u64,
}

impl Default for TextViewState {
    fn default() -> Self {
        Self {
            rope: Rope::from_str(""),
            scroll_offset: 0.0,
            target_scroll_offset: 0.0,
            horizontal_scroll_offset: 0.0,
            target_horizontal_scroll_offset: 0.0,
            needs_update: true,
            needs_scroll_update: false,
            pending_update: false,
            last_render_time: 0.0,
            content_version: 0,
            dirty_lines: None,
            previous_line_count: 1,
            max_content_width: 0.0,
            max_content_width_version: 0,
            max_width_line: None,
            line_width_tracker: LineWidthTracker::new(),
            styled_lines: Vec::new(),
            styled_lines_version: 0,
        }
    }
}

impl TextViewState {
    /// Create a new text view with initial text content
    pub fn with_text(text: &str) -> Self {
        let rope = Rope::from_str(text);
        let line_count = rope.len_lines();
        let line_width_tracker = LineWidthTracker::from_rope(&rope);

        Self {
            rope,
            previous_line_count: line_count,
            line_width_tracker,
            needs_update: true,
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
        self.dirty_lines = None; // Full invalidation
        self.needs_update = true;
        self.previous_line_count = self.rope.len_lines();
        self.line_width_tracker = LineWidthTracker::from_rope(&self.rope);
        self.styled_lines.clear();
        self.styled_lines_version += 1;
    }

    /// Mark a range of lines as dirty (needing re-render)
    pub fn mark_dirty(&mut self, range: Range<usize>) {
        if let Some(ref existing) = self.dirty_lines {
            let start = existing.start.min(range.start);
            let end = existing.end.max(range.end);
            self.dirty_lines = Some(start..end);
        } else {
            self.dirty_lines = Some(range);
        }
        self.content_version += 1;
        self.pending_update = true;
    }

    /// Mark all lines as dirty
    pub fn mark_all_dirty(&mut self) {
        self.dirty_lines = None;
        self.content_version += 1;
        self.pending_update = true;
    }

    /// Set styled segments for a specific line
    pub fn set_styled_line(&mut self, line: usize, segments: Vec<LineSegment>) {
        // Ensure capacity
        if self.styled_lines.len() <= line {
            self.styled_lines.resize_with(line + 1, || None);
        }
        self.styled_lines[line] = Some(segments);
        self.styled_lines_version += 1;
    }

    /// Clear all styled lines (forces re-computation)
    pub fn clear_styled_lines(&mut self) {
        self.styled_lines.clear();
        self.styled_lines_version += 1;
    }
}
