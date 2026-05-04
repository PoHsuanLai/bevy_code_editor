//! GPU-accelerated text rendering engine for Bevy.
//!
//! Provides the primitives for building text-heavy UIs (editors, terminals,
//! chat panels, log viewers): a glyph atlas, instanced GPU rendering, a
//! `DisplayLayout` snapshot type, and an overlay system. Knows nothing about
//! editing, cursors, syntax, or input — those belong to consumer crates.
//!
//! ## Layers
//!
//! - [`gpu`]: glyph atlas, instanced rendering pipeline, WGSL shaders.
//! - [`view`]: `TextView`, `TextViewState`, `DisplayLayout`, `ShapedLine`,
//!   `StyleRun`, `RectOverlay`, `render_layout`.
//!
//! ## Phase 1 status
//!
//! The crate is currently a relocation of the original code; subsequent
//! phases will add `FontConfig`, `TextEnginePlugin` / `TextEnginePlugins`,
//! and extend `StyleRun` / `ShapedLine` for rich-text consumers. The
//! singleton-flavored `extract_text_globals` extract path used to live in
//! `gpu::instanced_render`; it now lives in the editor crate which is the
//! only consumer that needs it.

pub mod gpu;
pub mod view;

pub use gpu::*;
pub use view::*;
