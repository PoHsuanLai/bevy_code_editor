//! Performance and rendering settings

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Performance settings
#[derive(Clone, Debug, Resource, Serialize, Deserialize)]
pub struct PerformanceSettings {
    /// Number of lines to buffer outside viewport for smoother scrolling
    pub viewport_buffer_lines: usize,

    /// Enable GPU-accelerated text rendering
    pub gpu_text: bool,
}

impl Default for PerformanceSettings {
    fn default() -> Self {
        Self {
            viewport_buffer_lines: 10,
            gpu_text: true,
        }
    }
}
