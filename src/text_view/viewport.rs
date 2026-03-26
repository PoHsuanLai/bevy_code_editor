//! TextViewViewport — per-instance viewport dimensions and layout

use bevy::prelude::*;

/// Viewport dimensions and layout for a single text view instance.
///
/// This is a Component (not a Resource) so each text view entity has its own viewport.
#[derive(Component, Clone, Copy, Debug)]
pub struct TextViewViewport {
    /// Viewport width in pixels
    pub width: u32,
    /// Viewport height in pixels
    pub height: u32,

    /// Horizontal offset for content center (useful for sidebars/panels)
    pub offset_x: f32,
    /// Vertical offset for content center (useful for panels)
    pub offset_y: f32,

    // === Computed Layout ===
    /// Left margin/padding before text starts
    pub text_area_left: f32,
    /// Top margin/padding before text starts
    pub text_area_top: f32,
    /// Width of the gutter area (line numbers, etc.) — 0 for non-editor views
    pub gutter_width: f32,
    /// X position of the separator line between gutter and code
    pub separator_x: f32,
}

impl Default for TextViewViewport {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            offset_x: 0.0,
            offset_y: 0.0,
            text_area_left: 0.0,
            text_area_top: 8.0,
            gutter_width: 0.0,
            separator_x: 0.0,
        }
    }
}

impl TextViewViewport {
    /// Calculate the world coordinate of the viewport's left edge
    pub fn world_left(&self) -> f32 {
        if self.offset_x == 0.0 && self.offset_y == 0.0 {
            -(self.width as f32) / 2.0
        } else {
            self.offset_x
        }
    }

    /// Calculate the world coordinate of the viewport's top edge
    pub fn world_top(&self) -> f32 {
        if self.offset_x == 0.0 && self.offset_y == 0.0 {
            self.height as f32 / 2.0
        } else {
            self.offset_y
        }
    }
}
