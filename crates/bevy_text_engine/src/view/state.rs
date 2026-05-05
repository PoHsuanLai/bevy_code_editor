//! `TextViewState` — generic text view state component.
//!
//! Holds the rope buffer, scroll offsets, and a content-version counter.
//! Reusable core shared by the code editor and any other scrollable text
//! view (chat panel, log viewer, etc.).

use bevy::prelude::*;
use ropey::Rope;

/// Generic text view state — holds the rope buffer and scroll state for a
/// scrollable text-rendering entity.
///
/// Rendering invalidation flows through the editor's `LayoutFingerprint`
/// (which reads `content_version` and scroll/viewport changes) and Bevy's
/// `Changed<DisplayLayout>` / `Changed<TextViewOverlays>` change detection.
/// There is no per-frame dirty-line bookkeeping here; mutators only need
/// to bump `content_version`.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct TextViewState {
    /// The text content (efficient rope data structure)
    // Rope is not Reflect — leave out of reflection.
    #[reflect(ignore)]
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

    /// Maximum content width seen so far (longest shaped line in pixels).
    /// Updated by the display-map producer per visible line; consumed by the
    /// horizontal scrollbar in `bevy_code_editor`.
    pub max_content_width: f32,
    /// The buffer line index that produced `max_content_width`.
    pub max_width_line: Option<usize>,
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
        }
    }
}

impl TextViewState {
    /// Create a new text view with initial text content
    pub fn with_text(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
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
        self.max_content_width = 0.0;
        self.max_width_line = None;
    }

    /// Bump `content_version` to force a layout rebuild on the next frame.
    /// Use after any rope mutation that didn't go through `set_text`.
    pub fn bump_version(&mut self) {
        self.content_version += 1;
    }
}
