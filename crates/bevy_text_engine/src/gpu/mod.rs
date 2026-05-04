//! GPU text rendering via instanced draw calls and a glyph atlas.
//!
//! 1. **Glyph Atlas** - Rasterizes glyphs to a texture atlas once
//! 2. **Instanced Rendering** - Renders all visible glyphs in a single draw call
//! 3. **Custom Shader** - WGSL shader for text rendering with per-glyph colors

mod atlas;
mod render;

mod instanced_render;

use bevy::prelude::*;

pub use atlas::{
    GlyphAtlas, GlyphInfo, GlyphKey, GlyphRasterizer, RasterizedGlyph, ATLAS_SIZE, DPI_SCALE,
};

pub use atlas::{PlacementInfo, PositionedGlyph};

pub use instanced_render::{ExtractedTextGlobals, InstancedTextRenderPlugin};

pub use atlas::GlyphAtlas as GlyphAtlasType;
pub use render::{
    update_atlas_texture, GlyphBatch, GpuTextPlugin, TextBatchBuilder, TextMaterial,
    TextRenderState,
};
// `gpu::render::GlyphInstance` is the legacy material-batch type used only
// inside `gpu/render.rs`. The live one is `view::render::GlyphInstance` —
// re-exported from the crate root via `view::*`.

pub use bevy::sprite_render::MeshMaterial2d;

/// System condition: true once the glyph atlas resource exists.
pub fn atlas_ready(atlas: Option<Res<GlyphAtlas>>) -> bool {
    atlas.is_some()
}
