//! Minimap settings
//!
//! ## Z-Index Layering
//!
//! The minimap uses Bevy's `GlobalZIndex` for proper layering. The default z-indices are:
//! - Background: 100
//! - Viewport Highlight: 200
//! - Text Content: 300
//!
//! Users can customize these values to integrate with their own UI z-index system.
//! Higher values render on top of lower values.
//!
//! Example:
//! ```ignore
//! minimap_settings.background_z_index = 50;
//! minimap_settings.viewport_highlight_z_index = 51;
//! minimap_settings.text_z_index = 52;
//! ```

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Minimap settings
#[derive(Clone, Debug, Resource, Serialize, Deserialize)]
pub struct MinimapSettings {
    /// Enable minimap
    pub enabled: bool,

    /// Minimap width in pixels
    pub width: f32,

    /// Minimap line height (pixels)
    pub line_height: f32,

    /// Minimap font size
    pub font_size: f32,

    /// Maximum column to render
    pub max_column: usize,

    /// Center minimap content when shorter than viewport
    pub center_when_short: bool,

    /// Show on right side
    pub show_on_right: bool,

    /// Padding from edge (left or right depending on show_on_right)
    pub edge_padding: f32,

    /// Show viewport highlight
    pub show_viewport_highlight: bool,

    /// Show slider
    pub show_slider: bool,

    /// Show slider only on hover
    pub slider_on_hover_only: bool,

    /// Minimum indicator height
    pub min_indicator_height: f32,

    /// Background Z-index (used for GlobalZIndex component)
    /// Default: 100
    pub background_z_index: i32,

    /// Text content Z-index (used for GlobalZIndex component)
    /// Should be higher than background and viewport highlight
    /// Default: 300
    pub text_z_index: i32,

    /// Viewport highlight Z-index (used for GlobalZIndex component)
    /// Should be between background and text
    /// Default: 200
    pub viewport_highlight_z_index: i32,

    /// Slider Z-index (deprecated, use viewport_highlight_z_index)
    pub slider_z_index: f32,

    /// Scrollbar width
    pub scrollbar_width: f32,

    /// Scrollbar spacing from minimap
    pub scrollbar_spacing: f32,

    /// Scrollbar minimum thumb height
    pub scrollbar_min_thumb_height: f32,

    /// Scrollbar Z-index
    pub scrollbar_z_index: f32,

    /// Scrollbar track color
    pub scrollbar_track_color: Color,

    /// Scrollbar thumb color
    pub scrollbar_thumb_color: Color,

    /// Scrollbar border radius
    pub scrollbar_border_radius: f32,

    /// Minimum viewport width to show minimap (auto-hide when narrower)
    /// Set to 0.0 to disable auto-hide
    pub min_viewport_width: f32,
}

impl Default for MinimapSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            width: 100.0,
            line_height: 4.0,
            font_size: 3.5,
            max_column: 120,
            center_when_short: true,
            show_on_right: true,
            edge_padding: 100.0,
            show_viewport_highlight: true,
            show_slider: true,
            slider_on_hover_only: false,
            min_indicator_height: 20.0,
            background_z_index: 100,
            text_z_index: 300,
            viewport_highlight_z_index: 200,
            slider_z_index: 5.1, // Deprecated
            scrollbar_width: 6.0,
            scrollbar_spacing: 2.0,
            scrollbar_min_thumb_height: 30.0,
            scrollbar_z_index: 5.15,
            scrollbar_track_color: Color::srgba(0.15, 0.15, 0.15, 0.5),
            scrollbar_thumb_color: Color::srgba(0.4, 0.4, 0.4, 0.7),
            scrollbar_border_radius: 3.0,
            min_viewport_width: 500.0, // Hide minimap when viewport < 500px wide
        }
    }
}

impl MinimapSettings {
    /// Check if minimap should be visible given the current viewport width
    pub fn should_show(&self, viewport_width: f32) -> bool {
        self.enabled
            && (self.min_viewport_width == 0.0 || viewport_width >= self.min_viewport_width)
    }
}
