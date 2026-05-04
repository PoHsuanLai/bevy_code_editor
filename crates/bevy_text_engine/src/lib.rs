//! GPU-accelerated text rendering engine for Bevy.
//!
//! Provides primitives for building text-heavy UIs (editors, terminals,
//! chat panels, log viewers): a glyph atlas, instanced GPU rendering, a
//! `DisplayLayout` snapshot type, and an overlay system. Knows nothing
//! about editing, cursors, syntax, or input — those belong to consumer
//! crates.
//!
//! - [`gpu`]: glyph atlas, instanced rendering pipeline, WGSL shaders.
//! - [`view`]: `TextView`, `TextViewState`, `DisplayLayout`, `ShapedLine`,
//!   `StyleRun`, `RectOverlay`, `render_layout`, `TextEnginePlugin`,
//!   `TextEnginePlugins`.
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
        DisplayLayout, FontConfig, TextEnginePlugin, TextEnginePlugins, TextView, TextViewState,
        TextViewViewport,
    };
}
