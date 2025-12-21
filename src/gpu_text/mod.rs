//! GPU Text Rendering Module
//!
//! High-performance text rendering using instanced GPU rendering.
//! Inspired by Zed's GPUI approach.
//!
//! ## Architecture
//!
//! 1. **Glyph Atlas** - Rasterizes glyphs to a texture atlas once
//! 2. **Instanced Rendering** - Renders all visible glyphs in a single draw call
//! 3. **Custom Shader** - WGSL shader for efficient text rendering with colors
//!
//! ## Usage
//!
//! ```rust,ignore
//! // Add the plugin
//! app.add_plugins(GpuTextPlugin);
//!
//! // Build a text batch
//! let mut batch = GlyphBatch::new();
//! batch.add_string("Hello, World!", 0.0, 0.0, 14.0, Color::WHITE, &mut atlas);
//! ```

mod atlas;
mod render;

pub use atlas::{GlyphAtlas, GlyphInfo, GlyphKey, GlyphRasterizer, RasterizedGlyph, ATLAS_SIZE};
pub use render::{
    GlyphBatch, GlyphInstance, GpuTextPlugin, TextBatchBuilder, TextMaterial, TextRenderState,
};

// Re-export from bevy for convenience
pub use bevy::sprite_render::MeshMaterial2d;
