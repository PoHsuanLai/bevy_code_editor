//! Minimap settings

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

    /// Background Z-index
    pub background_z_index: f32,

    /// Viewport highlight Z-index
    pub viewport_highlight_z_index: f32,

    /// Slider Z-index
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
            background_z_index: 5.0,
            viewport_highlight_z_index: 5.05,
            slider_z_index: 5.1,
            scrollbar_width: 6.0,
            scrollbar_spacing: 2.0,
            scrollbar_min_thumb_height: 30.0,
            scrollbar_z_index: 5.15,
            scrollbar_track_color: Color::srgba(0.15, 0.15, 0.15, 0.5),
            scrollbar_thumb_color: Color::srgba(0.4, 0.4, 0.4, 0.7),
            scrollbar_border_radius: 3.0,
        }
    }
}
