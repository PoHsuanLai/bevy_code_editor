//! Performance and rendering settings

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Performance settings
#[derive(Clone, Debug, Resource, Serialize, Deserialize, Reflect)]
#[reflect(Resource, Default, Debug)]
pub struct PerformanceSettings {
    /// Number of lines to buffer outside viewport for smoother scrolling
    pub viewport_buffer_lines: usize,

    /// Enable GPU-accelerated text rendering
    pub gpu_text: bool,

    /// Max milliseconds per frame for glyph building (cache misses).
    /// Prevents frame stalls when many lines need syntax highlighting at once.
    pub glyph_build_budget_ms: f64,
}

impl Default for PerformanceSettings {
    fn default() -> Self {
        Self {
            viewport_buffer_lines: 10,
            gpu_text: true,
            glyph_build_budget_ms: 8.0,
        }
    }
}
