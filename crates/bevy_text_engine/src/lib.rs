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
//!   `StyleRun`, `RectOverlay`, `render_layout`.

pub mod gpu;
pub mod view;

pub use gpu::*;
pub use view::*;
