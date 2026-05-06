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
pub mod ui;
pub mod view;

pub use gpu::*;
pub use ui::*;
pub use view::*;

pub mod prelude {
    //! Common types for spawning and rendering text views.
    pub use crate::gpu::{GlyphAtlasPlugin, InstancedTextRenderPlugin};
    pub use crate::ui::{Scrollbar, ScrollbarOrientation, ScrollbarPlugin, ScrollbarState};
    pub use crate::view::{
        Block, BlockDecorTheme, BlockLayoutConfig, BlockList, ContentMetrics, DisplayLayout,
        FontConfig, FontSynthesis, HiddenLines, LayoutWrap, LineStyles, RenderTheme, RunWithText,
        ScrollState, StyleRun, TextBuffer, TextEnginePlugin, TextEnginePlugins, TextView,
        TextViewViewport,
    };
}
