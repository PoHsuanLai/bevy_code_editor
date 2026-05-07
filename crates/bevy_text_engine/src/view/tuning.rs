//! Per-app rendering performance knobs.
//!
//! Defaults are tuned for a typical desktop IDE on a 1080p–4K monitor.
//! Hosts targeting unusual environments (mobile/embedded, multi-megabyte
//! files, paper-thin chat panels) override fields on [`TextEngineTuning`]:
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use bevy_text_engine::TextEngineTuning;
//!
//! App::new()
//!     .insert_resource(TextEngineTuning {
//!         viewport_buffer_lines: 8,
//!         shape_cache_capacity: 32_768,
//!     })
//!     .run();
//! ```
//!
//! Inserting before plugin setup is strongly preferred; [`shape_cache_capacity`]
//! sizes a `HashMap` capacity at atlas construction and changes after that
//! point have no effect.
//!
//! [`shape_cache_capacity`]: TextEngineTuning::shape_cache_capacity

use bevy::prelude::*;

/// Application-wide perf tunables for the text engine.
#[derive(Resource, Clone, Copy, Debug, Reflect)]
#[reflect(Resource, Default, Debug)]
pub struct TextEngineTuning {
    /// Extra rows kept above/below the visible window during layout. More
    /// = smoother fast-scroll into view, fewer mid-frame layout rebuilds;
    /// less = lower steady-state shaping cost on huge files.
    pub viewport_buffer_lines: u32,

    /// FIFO capacity of the per-line shape cache. Cosmic-text's
    /// `ShapeLine::new` is the dominant cost when scrolling big files
    /// (~1 ms / line), so a larger cache helps anyone working in
    /// 100k+-line buffers. Smaller is fine for chat / log viewers.
    pub shape_cache_capacity: usize,
}

impl Default for TextEngineTuning {
    fn default() -> Self {
        Self {
            viewport_buffer_lines: 4,
            shape_cache_capacity: 8192,
        }
    }
}
