//! Glyph Atlas - Caches rasterized glyphs in a GPU texture
//!
//! Uses cosmic_text (same as Zed) for high-quality font rasterization.
//! Glyphs are rasterized once and cached in a texture atlas for efficient GPU rendering.

use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use cosmic_text::{CacheKey, FontSystem, SwashCache};
use std::collections::HashMap;

/// Size of the glyph atlas texture (power of 2 for GPU efficiency)
pub const ATLAS_SIZE: u32 = 2048;

/// Padding between glyphs to prevent bleeding
const GLYPH_PADDING: u32 = 2;

/// A unique identifier for a cached glyph
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    /// The character
    pub character: char,
    /// Font size in pixels (scaled by 10 for sub-pixel precision)
    pub font_size_tenths: u32,
}

impl GlyphKey {
    pub fn new(character: char, font_size: f32) -> Self {
        Self {
            character,
            font_size_tenths: (font_size * 10.0) as u32,
        }
    }
}

/// Information about a glyph's location in the atlas
#[derive(Clone, Copy, Debug)]
pub struct GlyphInfo {
    /// UV coordinates in the atlas (0.0 to 1.0)
    pub uv_min: Vec2,
    pub uv_max: Vec2,
    /// Size in pixels
    pub size: Vec2,
    /// Offset from the baseline
    pub offset: Vec2,
    /// Advance width (how far to move for next character)
    pub advance: f32,
}

/// Row-based packing for the atlas (simple shelf algorithm)
struct AtlasRow {
    y: u32,
    height: u32,
    x_cursor: u32,
}

/// The glyph atlas resource
#[derive(Resource)]
pub struct GlyphAtlas {
    /// The atlas texture handle
    pub texture: Handle<Image>,
    /// Cached glyph information
    glyphs: HashMap<GlyphKey, GlyphInfo>,
    /// Current packing rows
    rows: Vec<AtlasRow>,
    /// Current Y position for new rows
    current_y: u32,
    /// Raw pixel data for CPU-side updates
    pixels: Vec<u8>,
    /// Whether the texture needs to be updated
    pub dirty: bool,
    /// Font system for text rasterization
    font_system: FontSystem,
    /// Swash cache for glyph rasterization
    swash_cache: SwashCache,
}

impl GlyphAtlas {
    /// Create a new glyph atlas
    pub fn new(images: &mut Assets<Image>) -> Self {
        // Create RGBA texture
        let pixels = vec![0u8; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize];

        let image = Image::new(
            Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            pixels.clone(),
            TextureFormat::Rgba8UnormSrgb,
            default(),
        );

        let texture = images.add(image);

        // Initialize cosmic_text font system
        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();

        Self {
            texture,
            glyphs: HashMap::new(),
            rows: Vec::new(),
            current_y: 0,
            pixels,
            dirty: false,
            font_system,
            swash_cache,
        }
    }

    /// Get or create a glyph entry in the atlas
    pub fn get_or_insert(&mut self, key: GlyphKey, rasterize: impl FnOnce() -> Option<RasterizedGlyph>) -> Option<GlyphInfo> {
        if let Some(info) = self.glyphs.get(&key) {
            return Some(*info);
        }

        // Try cosmic_text rasterization first, fall back to provided rasterizer
        let glyph = self.rasterize_with_cosmic(key).or_else(rasterize)?;

        // Find space in the atlas
        let (x, y) = self.allocate(glyph.width, glyph.height)?;

        // Copy glyph pixels to atlas
        self.copy_glyph_to_atlas(x, y, &glyph);

        // Calculate UV coordinates
        let uv_min = Vec2::new(
            x as f32 / ATLAS_SIZE as f32,
            y as f32 / ATLAS_SIZE as f32,
        );
        let uv_max = Vec2::new(
            (x + glyph.width) as f32 / ATLAS_SIZE as f32,
            (y + glyph.height) as f32 / ATLAS_SIZE as f32,
        );

        let info = GlyphInfo {
            uv_min,
            uv_max,
            size: Vec2::new(glyph.width as f32, glyph.height as f32),
            offset: Vec2::new(glyph.bearing_x, glyph.bearing_y),
            advance: glyph.advance,
        };

        self.glyphs.insert(key, info);
        self.dirty = true;

        // Debug first few glyphs
        if self.glyphs.len() <= 3 {
            println!("Atlas: Added glyph '{}' with size {:?}, uv {:?}-{:?}",
                     key.character, info.size, info.uv_min, info.uv_max);
        }

        Some(info)
    }

    /// Rasterize a glyph using cosmic_text/swash
    fn rasterize_with_cosmic(&mut self, key: GlyphKey) -> Option<RasterizedGlyph> {
        let font_size = key.font_size_tenths as f32 / 10.0;
        let character = key.character;

        // Skip control characters
        if character.is_control() && character != '\t' {
            return None;
        }

        // Get a font that supports this character
        // cosmic_text's font_system provides access to system fonts
        let font_id = {
            let db = self.font_system.db();
            // Find a font that has this glyph
            db.faces().find_map(|face| {
                // Try to find a monospace font first for code editing
                if face.monospaced {
                    Some(face.id)
                } else {
                    None
                }
            }).or_else(|| {
                // Fall back to any font
                db.faces().next().map(|f| f.id)
            })?
        };

        // Get the font
        let font = self.font_system.get_font(font_id)?;
        let swash_font = font.as_swash();

        // Get glyph ID for this character
        let glyph_id = swash_font.charmap().map(character);
        if glyph_id == 0 && character != ' ' {
            // No glyph for this character, try fallback
            return None;
        }

        // Get glyph metrics
        let metrics = swash_font.glyph_metrics(&[]).scale(font_size);
        let advance = metrics.advance_width(glyph_id);

        // Create cache key for swash
        let cache_key = CacheKey::new(
            font_id,
            glyph_id,
            font_size,
            (0.0, 0.0), // No subpixel offset
            cosmic_text::CacheKeyFlags::empty(),
        );

        // Get the rasterized image
        let image = self.swash_cache.get_image_uncached(&mut self.font_system, cache_key.0)?;

        // Handle empty glyphs (like space)
        if image.placement.width == 0 || image.placement.height == 0 {
            return Some(RasterizedGlyph {
                width: 0,
                height: 0,
                bearing_x: 0.0,
                bearing_y: 0.0,
                advance,
                pixels: Vec::new(),
            });
        }

        // Convert to our format
        let width = image.placement.width;
        let height = image.placement.height;
        let bearing_x = image.placement.left as f32;
        let bearing_y = image.placement.top as f32;

        // The image data is in coverage format (single channel alpha)
        // We need to extract just the alpha values
        let pixels = match image.content {
            cosmic_text::SwashContent::Mask => {
                // Already grayscale alpha
                image.data.clone()
            }
            cosmic_text::SwashContent::Color => {
                // RGBA, extract alpha channel
                image.data.chunks(4).map(|pixel| pixel[3]).collect()
            }
            cosmic_text::SwashContent::SubpixelMask => {
                // Subpixel rendering, convert to grayscale
                image.data.chunks(3).map(|pixel| {
                    ((pixel[0] as u16 + pixel[1] as u16 + pixel[2] as u16) / 3) as u8
                }).collect()
            }
        };

        Some(RasterizedGlyph {
            width,
            height,
            bearing_x,
            bearing_y,
            advance,
            pixels,
        })
    }

    /// Allocate space in the atlas using shelf packing
    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        if width == 0 || height == 0 {
            return Some((0, 0));
        }

        let padded_width = width + GLYPH_PADDING;
        let padded_height = height + GLYPH_PADDING;

        // Try to fit in an existing row
        for row in &mut self.rows {
            if row.height >= padded_height && row.x_cursor + padded_width <= ATLAS_SIZE {
                let x = row.x_cursor;
                let y = row.y;
                row.x_cursor += padded_width;
                return Some((x, y));
            }
        }

        // Create a new row
        if self.current_y + padded_height <= ATLAS_SIZE {
            let y = self.current_y;
            self.current_y += padded_height;
            self.rows.push(AtlasRow {
                y,
                height: padded_height,
                x_cursor: padded_width,
            });
            return Some((0, y));
        }

        // Atlas is full
        None
    }

    /// Copy glyph pixels to the atlas
    fn copy_glyph_to_atlas(&mut self, x: u32, y: u32, glyph: &RasterizedGlyph) {
        if glyph.width == 0 || glyph.height == 0 {
            return;
        }

        for gy in 0..glyph.height {
            for gx in 0..glyph.width {
                let src_idx = (gy * glyph.width + gx) as usize;
                let dst_x = x + gx;
                let dst_y = y + gy;
                let dst_idx = ((dst_y * ATLAS_SIZE + dst_x) * 4) as usize;

                if dst_idx + 3 < self.pixels.len() && src_idx < glyph.pixels.len() {
                    let alpha = glyph.pixels[src_idx];
                    // Store as white with alpha (for colored text)
                    self.pixels[dst_idx] = 255;     // R
                    self.pixels[dst_idx + 1] = 255; // G
                    self.pixels[dst_idx + 2] = 255; // B
                    self.pixels[dst_idx + 3] = alpha; // A
                }
            }
        }
    }

    /// Update the GPU texture with any changes
    pub fn update_texture(&mut self, images: &mut Assets<Image>) {
        if !self.dirty {
            return;
        }

        if let Some(image) = images.get_mut(&self.texture) {
            image.data = Some(self.pixels.clone());
        }

        self.dirty = false;
    }

    /// Clear the atlas (e.g., when font changes)
    pub fn clear(&mut self) {
        self.glyphs.clear();
        self.rows.clear();
        self.current_y = 0;
        self.pixels.fill(0);
        self.dirty = true;
    }

    /// Check if a glyph is cached
    pub fn contains(&self, key: &GlyphKey) -> bool {
        self.glyphs.contains_key(key)
    }

    /// Get cached glyph info
    pub fn get(&self, key: &GlyphKey) -> Option<&GlyphInfo> {
        self.glyphs.get(key)
    }
}

/// A rasterized glyph ready to be copied to the atlas
pub struct RasterizedGlyph {
    pub width: u32,
    pub height: u32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub advance: f32,
    /// Grayscale pixels (alpha values)
    pub pixels: Vec<u8>,
}

/// Fallback software glyph rasterizer
/// Used when cosmic_text doesn't have the font/glyph
pub struct GlyphRasterizer;

impl GlyphRasterizer {
    /// Rasterize a character to a bitmap (fallback)
    pub fn rasterize(character: char, font_size: f32) -> Option<RasterizedGlyph> {
        // Skip control characters
        if character.is_control() && character != '\t' {
            return None;
        }

        // Simple monospace approximation for fallback
        let char_width = (font_size * 0.6).ceil() as u32;
        let char_height = font_size.ceil() as u32;

        // Create a simple filled rectangle (placeholder)
        let pixels = vec![200u8; (char_width * char_height) as usize];

        Some(RasterizedGlyph {
            width: char_width.max(1),
            height: char_height.max(1),
            bearing_x: 0.0,
            bearing_y: font_size * 0.8,
            advance: char_width as f32,
            pixels,
        })
    }
}
