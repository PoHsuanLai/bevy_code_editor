//! Scrollbar settings

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Scrollbar settings
#[derive(Clone, Debug, Resource, Serialize, Deserialize)]
pub struct ScrollbarSettings {
    /// Enable scrollbar
    pub enabled: bool,

    /// Scrollbar width in pixels
    pub width: f32,

    /// Scrollbar background color
    pub background_color: Color,

    /// Scrollbar thumb color
    pub thumb_color: Color,

    /// Scrollbar thumb hover color
    pub thumb_hover_color: Color,

    /// Auto-hide when not hovering
    pub auto_hide: bool,

    /// Fade duration (seconds) for auto-hide
    pub fade_duration: f32,
}

impl Default for ScrollbarSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            width: 12.0,
            background_color: Color::srgba(0.2, 0.2, 0.2, 0.3),
            thumb_color: Color::srgba(0.5, 0.5, 0.5, 0.5),
            thumb_hover_color: Color::srgba(0.6, 0.6, 0.6, 0.7),
            auto_hide: false,
            fade_duration: 0.3,
        }
    }
}
