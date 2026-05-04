//! Scrolling behavior settings

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Scrolling settings
#[derive(Clone, Debug, Resource, Serialize, Deserialize)]
pub struct ScrollingSettings {
    /// Scroll speed multiplier
    pub speed: f32,

    /// Enable smooth scrolling
    pub smooth: bool,

    /// Smooth scroll duration (seconds)
    pub smooth_duration: f32,

    /// Keep cursor visible when scrolling (pixels from edge)
    pub cursor_margin: f32,
}

impl Default for ScrollingSettings {
    fn default() -> Self {
        Self {
            speed: 3.0,
            smooth: true,
            smooth_duration: 0.15,
            cursor_margin: 50.0,
        }
    }
}
