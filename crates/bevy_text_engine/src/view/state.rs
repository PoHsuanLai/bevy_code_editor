//! Rope buffer + scroll state for a scrollable text-rendering entity.

use bevy::prelude::*;
use ropey::Rope;

/// Rope buffer, scroll offsets, and content version for a text view entity.
/// Mutators only need to bump `content_version`; invalidation flows through
/// `Changed<DisplayLayout>`.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct TextViewState {
    // Rope is not Reflect — excluded from reflection.
    #[reflect(ignore)]
    pub rope: Rope,

    pub scroll_offset: f32,
    pub target_scroll_offset: f32,
    pub horizontal_scroll_offset: f32,
    pub target_horizontal_scroll_offset: f32,

    /// Bumped on every rope mutation; the display-map fingerprint reads this
    /// to decide whether to rebuild the layout.
    pub content_version: u64,

    /// Longest shaped line in pixels — consumed by the horizontal scrollbar.
    pub max_content_width: f32,
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
    pub fn with_text(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
            content_version: 1,
            ..Default::default()
        }
    }

    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

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
