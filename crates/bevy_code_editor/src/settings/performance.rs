//! Performance and rendering settings

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Rendering performance tuning. Controls viewport culling buffer size,
/// whether the GPU text pipeline is active, and per-frame glyph build budget.
#[derive(Clone, Debug, Resource, Serialize, Deserialize, Reflect)]
#[reflect(Resource, Default, Debug)]
pub struct PerformanceSettings {
    pub viewport_buffer_lines: usize,
    pub gpu_text: bool,
    /// Max ms per frame for glyph building; prevents stalls on many cache misses.
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
