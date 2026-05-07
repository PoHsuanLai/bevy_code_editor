//! GPU-accelerated text rendering engine for Bevy.
//!
//! Provides primitives for building text-heavy UIs (editors, terminals,
//! chat panels, log viewers): a glyph atlas, instanced GPU rendering, a
//! `DisplayLayout` snapshot type, and an overlay system. Knows nothing
//! about editing, cursors, syntax, or input — those belong to consumer
//! crates.
//!
//! - [`gpu`]: glyph atlas, instanced rendering pipeline, WGSL shaders.
//! - [`view`]: `TextView`, `TextBuffer`, `ScrollState`, `ContentMetrics`,
//!   `DisplayLayout`, `ShapedLine`, `StyleRun`, `RectOverlay`,
//!   `render_layout`, `TextEnginePlugin`, `TextEnginePlugins`.
//!
//! Scroll *state* (offset, content size, viewport size) is exposed via
//! `ScrollState` and `ContentMetrics` on each `TextView` entity. The engine
//! does not render its own scrollbar — hosts wire up `bevy_ui` scrollbars,
//! custom overlays, or no scrollbar at all.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use bevy_text_engine::prelude::*;
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(TextEnginePlugins)
//!     .run();
//! ```

pub mod gpu;
pub mod view;

pub use gpu::*;
pub use view::*;

pub mod prelude {
    //! Common types for spawning and rendering text views.
    pub use crate::gpu::{GlyphAtlasPlugin, InstancedTextRenderPlugin};
    pub use crate::view::{
        row_metrics, row_metrics_with_baseline, Block, BlockDecorTheme, BlockLayoutConfig,
        BlockList, ContentMetrics, DisplayLayout, FontConfig, FontSynthesis, HiddenLines,
        LayoutWrap, LineStyles, RenderTheme, RowMetrics, RowMetricsParam, RunWithText,
        ScrollState, StyleRun, TextBuffer, TextEnginePlugin, TextEnginePlugins, TextView,
        TextViewViewport,
    };
}
