//! Glyph atlas: rasterizes glyphs once via cosmic_text and caches them in a GPU texture.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use cosmic_text::{CacheKey, FontSystem, SwashCache};
use std::collections::HashMap;

/// Power of 2 for GPU efficiency.
pub const ATLAS_SIZE: u32 = 2048;

const GLYPH_PADDING: u32 = 2;

/// Rasterize at 2x for crisp text on HiDPI displays.
pub const DPI_SCALE: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    pub character: char,
    /// Scaled by 10 for sub-pixel precision.
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

#[derive(Clone, Copy, Debug)]
pub struct GlyphInfo {
    /// UV coordinates in the atlas (0.0 to 1.0)
    pub uv_min: Vec2,
    pub uv_max: Vec2,
    /// Size in logical pixels (atlas stores high-res, rendering uses logical size)
    pub size: Vec2,
    /// Offset from the baseline in logical pixels
    pub offset: Vec2,
    pub advance: f32,
}

/// Shelf-based row packing for the atlas.
struct AtlasRow {
    y: u32,
    height: u32,
    x_cursor: u32,
}

#[derive(Resource)]
pub struct GlyphAtlas {
    pub texture: Handle<Image>,
    glyphs: HashMap<GlyphKey, GlyphInfo>,
    rows: Vec<AtlasRow>,
    current_y: u32,
    pixels: Vec<u8>,
    pub dirty: bool,
    font_system: FontSystem,
    swash_cache: SwashCache,
    configured_font_id: Option<cosmic_text::fontdb::ID>,
    /// Cache keyed by cosmic_text CacheKey (used by instanced rendering path)
    cache: HashMap<cosmic_text::CacheKey, GlyphInfo>,
    /// Generation counter — incremented on atlas clear for cache invalidation
    pub generation: u64,
    /// Dirty row range for partial texture upload (min_y..max_y in pixels)
    dirty_min_y: u32,
    dirty_max_y: u32,
    /// UV info for a solid white pixel — used for background rectangles
    pub solid_uv: GlyphInfo,
}

impl GlyphAtlas {
    pub fn new(images: &mut Assets<Image>) -> Self {
        Self::new_with_font(images, None)
    }

    /// `font_path` can be a file path ("fonts/FiraMono-Regular.ttf") or family name ("Fira Mono").
    pub fn new_with_font(images: &mut Assets<Image>, font_path: Option<&str>) -> Self {
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
            // Keep in both worlds so we can update the data and have it re-upload
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );

        let texture = images.add(image);

        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();

        let configured_font_id = if let Some(path) = font_path {
            Self::find_or_load_font(&mut font_system, path)
        } else {
            None
        };

        let mut atlas = Self {
            texture,
            glyphs: HashMap::new(),
            rows: Vec::new(),
            current_y: 0,
            pixels,
            dirty: false,
            font_system,
            swash_cache,
            configured_font_id,
            cache: HashMap::new(),
            generation: 0,
            dirty_min_y: ATLAS_SIZE,
            dirty_max_y: 0,
            solid_uv: GlyphInfo {
                uv_min: Vec2::ZERO,
                uv_max: Vec2::ZERO,
                size: Vec2::ONE,
                offset: Vec2::ZERO,
                advance: 0.0,
            },
        };

        atlas.reserve_solid_pixel();
        atlas.dirty = true;
        atlas.dirty_min_y = 0;
        atlas.dirty_max_y = 2;

        atlas
    }

    fn find_or_load_font(
        font_system: &mut FontSystem,
        font_path: &str,
    ) -> Option<cosmic_text::fontdb::ID> {
        if font_path.ends_with(".ttf") || font_path.ends_with(".otf") {
            let paths_to_try = [
                font_path.to_string(),
                format!("assets/{}", font_path),
                format!("./{}", font_path),
            ];

            for path in &paths_to_try {
                if let Ok(data) = std::fs::read(path) {
                    let db = font_system.db_mut();
                    let count_before = db.faces().count();
                    db.load_font_data(data);
                    let count_after = db.faces().count();

                    if count_after > count_before {
                        if let Some(face) = db.faces().last() {
                            let family_name = face
                                .families
                                .first()
                                .map(|f| f.0.as_str())
                                .unwrap_or("Unknown");
                            info!("GPU Text: Loaded font '{}' from {}", family_name, path);
                            return Some(face.id);
                        }
                    }
                }
            }
        }

        // Extract family name from path if it looks like a path
        let family_name = if font_path.contains('/') || font_path.contains('\\') {
            std::path::Path::new(font_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| {
                    // Convert "FiraMono-Regular" to "Fira Mono"
                    s.split('-')
                        .next()
                        .unwrap_or(s)
                        .chars()
                        .fold(String::new(), |mut acc, c| {
                            if c.is_uppercase() && !acc.is_empty() && !acc.ends_with(' ') {
                                acc.push(' ');
                            }
                            acc.push(c);
                            acc
                        })
                })
                .unwrap_or_else(|| font_path.to_string())
        } else {
            font_path.to_string()
        };

        // Search for the font by family name (case-insensitive)
        let family_lower = family_name.to_lowercase();
        let db = font_system.db();

        if let Some(id) = db.faces().find_map(|face| {
            for family in &face.families {
                if family.0.to_lowercase().contains(&family_lower) {
                    info!("GPU Text: Using system font '{}'", family.0);
                    return Some(face.id);
                }
            }
            None
        }) {
            return Some(id);
        }

        // Fall back to any monospace font
        warn!(
            "GPU Text: Could not find font '{}', using system monospace fallback",
            font_path
        );
        None
    }

    /// Get or create a glyph entry in the atlas
    pub fn get_or_insert(
        &mut self,
        key: GlyphKey,
        rasterize: impl FnOnce() -> Option<RasterizedGlyph>,
    ) -> Option<GlyphInfo> {
        if let Some(info) = self.glyphs.get(&key) {
            return Some(*info);
        }

        // Try cosmic_text rasterization first, fall back to provided rasterizer
        let glyph = self.rasterize_with_cosmic(key).or_else(rasterize)?;

        // Find space in the atlas, with generation-based recovery on full
        let (x, y) = match self.allocate(glyph.width, glyph.height) {
            Some(pos) => pos,
            None => {
                warn!(
                    "Glyph atlas full, clearing and retrying (generation {})",
                    self.generation
                );
                self.clear();
                self.allocate(glyph.width, glyph.height)?
            }
        };

        // Copy glyph pixels to atlas
        self.copy_glyph_to_atlas(x, y, &glyph);

        // Calculate UV coordinates
        let uv_min = Vec2::new(x as f32 / ATLAS_SIZE as f32, y as f32 / ATLAS_SIZE as f32);
        let uv_max = Vec2::new(
            (x + glyph.width) as f32 / ATLAS_SIZE as f32,
            (y + glyph.height) as f32 / ATLAS_SIZE as f32,
        );

        // Size is scaled down to logical pixels for rendering
        // The atlas stores high-res glyphs, but we render at logical size
        let info = GlyphInfo {
            uv_min,
            uv_max,
            size: Vec2::new(
                glyph.width as f32 / DPI_SCALE,
                glyph.height as f32 / DPI_SCALE,
            ),
            offset: Vec2::new(glyph.bearing_x, glyph.bearing_y),
            advance: glyph.advance,
        };

        self.glyphs.insert(key, info);
        self.dirty = true;

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

        // Use configured font if available, otherwise fall back to system monospace
        let font_id = if let Some(id) = self.configured_font_id {
            id
        } else {
            let db = self.font_system.db();
            // Find a monospace font first for code editing
            db.faces()
                .find_map(|face| if face.monospaced { Some(face.id) } else { None })
                .or_else(|| {
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

        // Rasterize at higher resolution for crisp text on HiDPI displays
        let scaled_font_size = font_size * DPI_SCALE;

        // Get glyph metrics at the scaled size
        let metrics = swash_font.glyph_metrics(&[]).scale(scaled_font_size);
        let scaled_advance = metrics.advance_width(glyph_id);
        // Return advance at original size for layout
        let advance = scaled_advance / DPI_SCALE;

        // Create cache key for swash with scaled font size
        let cache_key = CacheKey::new(
            font_id,
            glyph_id,
            scaled_font_size,
            (0.0, 0.0), // No subpixel offset
            cosmic_text::CacheKeyFlags::empty(),
        );

        // Get the rasterized image at higher resolution
        let image = self
            .swash_cache
            .get_image_uncached(&mut self.font_system, cache_key.0)?;

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

        // Convert to our format - keep the high-res size for the atlas
        let width = image.placement.width;
        let height = image.placement.height;
        // Scale bearings back to original size for correct positioning
        let bearing_x = image.placement.left as f32 / DPI_SCALE;
        let bearing_y = image.placement.top as f32 / DPI_SCALE;

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
                image
                    .data
                    .chunks(3)
                    .map(|pixel| ((pixel[0] as u16 + pixel[1] as u16 + pixel[2] as u16) / 3) as u8)
                    .collect()
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

    /// Copy glyph pixels to the atlas, tracking dirty rect for partial upload
    fn copy_glyph_to_atlas(&mut self, x: u32, y: u32, glyph: &RasterizedGlyph) {
        if glyph.width == 0 || glyph.height == 0 {
            return;
        }

        // Track dirty rect
        self.dirty_min_y = self.dirty_min_y.min(y);
        self.dirty_max_y = self.dirty_max_y.max(y + glyph.height);

        for gy in 0..glyph.height {
            for gx in 0..glyph.width {
                let src_idx = (gy * glyph.width + gx) as usize;
                let dst_x = x + gx;
                let dst_y = y + gy;
                let dst_idx = ((dst_y * ATLAS_SIZE + dst_x) * 4) as usize;

                if dst_idx + 3 < self.pixels.len() && src_idx < glyph.pixels.len() {
                    let alpha = glyph.pixels[src_idx];
                    self.pixels[dst_idx] = 255; // R
                    self.pixels[dst_idx + 1] = 255; // G
                    self.pixels[dst_idx + 2] = 255; // B
                    self.pixels[dst_idx + 3] = alpha; // A
                }
            }
        }
    }

    /// Update the GPU texture with only the dirty rows (partial upload)
    pub fn update_texture(&mut self, images: &mut Assets<Image>) {
        if !self.dirty || self.dirty_min_y >= self.dirty_max_y {
            self.dirty = false;
            return;
        }

        let min_y = self.dirty_min_y.min(ATLAS_SIZE);
        let max_y = self.dirty_max_y.min(ATLAS_SIZE);

        if let Some(image) = images.get_mut(&self.texture) {
            if let Some(ref mut data) = image.data {
                // Partial copy: only update dirty rows
                let row_bytes = (ATLAS_SIZE * 4) as usize;
                let start_byte = min_y as usize * row_bytes;
                let end_byte = max_y as usize * row_bytes;

                if end_byte <= data.len() && end_byte <= self.pixels.len() {
                    data[start_byte..end_byte].copy_from_slice(&self.pixels[start_byte..end_byte]);
                } else {
                    // Fallback: full copy
                    data.copy_from_slice(&self.pixels);
                }
            }
        } else {
            // No existing image — create fresh (first frame)
            let new_image = Image::new(
                Extent3d {
                    width: ATLAS_SIZE,
                    height: ATLAS_SIZE,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                self.pixels.clone(),
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
            );
            let _ = images.insert(&self.texture, new_image);
        }

        // Reset dirty rect
        self.dirty = false;
        self.dirty_min_y = ATLAS_SIZE;
        self.dirty_max_y = 0;
    }

    /// Clear the atlas (e.g., when font changes or atlas is full)
    pub fn clear(&mut self) {
        self.glyphs.clear();
        self.rows.clear();
        self.current_y = 0;
        self.pixels.fill(0);
        self.dirty = true;
        self.cache.clear();
        self.generation += 1;
        self.dirty_min_y = 0;
        self.dirty_max_y = ATLAS_SIZE;
        // Re-reserve the solid white pixel for background fills
        self.reserve_solid_pixel();
    }

    /// Reserve a 2×2 white pixel region for solid-fill backgrounds.
    fn reserve_solid_pixel(&mut self) {
        if let Some((sx, sy)) = self.allocate(2, 2) {
            for dy in 0..2u32 {
                for dx in 0..2u32 {
                    let idx = (((sy + dy) * ATLAS_SIZE + sx + dx) * 4) as usize;
                    self.pixels[idx] = 255;
                    self.pixels[idx + 1] = 255;
                    self.pixels[idx + 2] = 255;
                    self.pixels[idx + 3] = 255;
                }
            }
            self.solid_uv = GlyphInfo {
                uv_min: Vec2::new(
                    (sx as f32 + 0.5) / ATLAS_SIZE as f32,
                    (sy as f32 + 0.5) / ATLAS_SIZE as f32,
                ),
                uv_max: Vec2::new(
                    (sx as f32 + 1.5) / ATLAS_SIZE as f32,
                    (sy as f32 + 1.5) / ATLAS_SIZE as f32,
                ),
                size: Vec2::ONE,
                offset: Vec2::ZERO,
                advance: 0.0,
            };
        }
    }

    /// Check if a glyph is cached
    pub fn contains(&self, key: &GlyphKey) -> bool {
        self.glyphs.contains_key(key)
    }

    /// Get cached glyph info
    pub fn get(&self, key: &GlyphKey) -> Option<&GlyphInfo> {
        self.glyphs.get(key)
    }

    /// Measure the advance width of a character
    pub fn measure_char_width(&mut self, character: char, font_size: f32) -> Option<f32> {
        let key = GlyphKey::new(character, font_size);
        // Force rasterize/load if not present to ensure we get a valid measurement
        self.get_or_insert(key, || GlyphRasterizer::rasterize(character, font_size))
            .map(|info| info.advance)
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

// ============================================================================
// INSTANCED RENDERING EXTENSIONS
// ============================================================================

pub use instanced_extensions::*;

mod instanced_extensions {
    use super::*;
    use cosmic_text::{Attrs, AttrsList, ShapeBuffer, ShapeLine, Shaping};

    /// Positioned glyph from cosmic_text layout
    #[derive(Clone, Copy, Debug)]
    pub struct PositionedGlyph {
        pub glyph_id: u16,
        pub x: f32,
        pub y: f32,
        pub font_id: cosmic_text::fontdb::ID,
        pub byte_index: usize,
    }

    /// Placement information for a rasterized glyph
    #[derive(Clone, Copy, Debug)]
    pub struct PlacementInfo {
        pub left: f32,
        pub top: f32,
    }

    impl GlyphAtlas {
        /// Pack a glyph into the atlas (simplified wrapper around get_or_insert)
        pub(crate) fn pack(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
            self.allocate(width, height)
        }

        /// Write glyph data to the atlas
        pub(crate) fn write_glyph_data(
            &mut self,
            x: u32,
            y: u32,
            width: u32,
            height: u32,
            data: &[u8],
        ) {
            if width == 0 || height == 0 {
                return;
            }

            for gy in 0..height {
                for gx in 0..width {
                    let src_idx = ((gy * width + gx) * 4) as usize;
                    let dst_x = x + gx;
                    let dst_y = y + gy;
                    let dst_idx = ((dst_y * ATLAS_SIZE + dst_x) * 4) as usize;

                    if dst_idx + 3 < self.pixels.len() && src_idx + 3 < data.len() {
                        self.pixels[dst_idx] = data[src_idx]; // R
                        self.pixels[dst_idx + 1] = data[src_idx + 1]; // G
                        self.pixels[dst_idx + 2] = data[src_idx + 2]; // B
                        self.pixels[dst_idx + 3] = data[src_idx + 3]; // A
                    }
                }
            }

            // Track dirty rect and mark as dirty
            self.dirty_min_y = self.dirty_min_y.min(y);
            self.dirty_max_y = self.dirty_max_y.max(y + height);
            self.dirty = true;
        }

        /// Layout a line of text using cosmic_text
        pub fn layout_line(&mut self, text: &str, font_size: f32) -> Vec<PositionedGlyph> {
            let attrs = Attrs::new();
            let attrs_list = AttrsList::new(attrs);

            let line = ShapeLine::new(
                &mut self.font_system,
                text,
                &attrs_list,
                Shaping::Advanced,
                4, // Tab width
            );

            let mut layout_lines = Vec::with_capacity(1);
            let mut scratch = ShapeBuffer::default();

            line.layout_to_buffer(
                &mut scratch,
                font_size,
                None,
                cosmic_text::Wrap::None,
                None,
                &mut layout_lines,
                None,
            );

            if layout_lines.is_empty() {
                return Vec::new();
            }

            let layout = &layout_lines[0];
            let mut result = Vec::with_capacity(layout.glyphs.len());

            for glyph in &layout.glyphs {
                result.push(PositionedGlyph {
                    glyph_id: glyph.glyph_id,
                    x: glyph.x,
                    y: glyph.y,
                    font_id: glyph.font_id,
                    byte_index: glyph.start,
                });
            }

            result
        }

        /// Get or rasterize a glyph using cosmic_text cache key
        pub fn get_or_rasterize_glyph(
            &mut self,
            cache_key: cosmic_text::CacheKey,
        ) -> Option<(GlyphInfo, PlacementInfo)> {
            use swash::scale::image::Content;

            // Check cache first
            if let Some(info) = self.cache.get(&cache_key) {
                // We don't cache PlacementInfo because it's specific to the layout engine's temporary cache
                // But we can reconstruct it from the info or just re-query the image if placement is needed separately.
                // Actually, the current usage in update_gpu_text_instanced needs PlacementInfo.
                // The `GlyphInfo` struct stores `offset` which is basically placement.
                // Let's see:
                // GlyphInfo.offset = Vec2(left / scale, top / scale)
                // PlacementInfo.left = left / scale, PlacementInfo.top = top / scale
                // So we can reconstruct PlacementInfo from GlyphInfo.offset!

                let placement = PlacementInfo {
                    left: info.offset.x,
                    top: info.offset.y,
                };
                return Some((*info, placement));
            }

            let image = self
                .swash_cache
                .get_image(&mut self.font_system, cache_key)
                .clone()?;

            if image.placement.width == 0 || image.placement.height == 0 {
                return None;
            }

            let width = image.placement.width as usize;
            let height = image.placement.height as usize;

            // Convert image data to RGBA based on content type
            let mut rgba_data = Vec::with_capacity(width * height * 4);
            match image.content {
                Content::Mask => {
                    for &alpha in &image.data {
                        rgba_data.extend_from_slice(&[255, 255, 255, alpha]);
                    }
                }
                Content::SubpixelMask | Content::Color => {
                    rgba_data.extend_from_slice(&image.data);
                }
            }

            // Pack into atlas, with generation-based recovery on full
            let pack_result = self.pack(width as u32, height as u32).or_else(|| {
                warn!(
                    "Glyph atlas full in get_or_rasterize_glyph, clearing (generation {})",
                    self.generation
                );
                self.clear();
                self.pack(width as u32, height as u32)
            });
            if let Some((x, y)) = pack_result {
                self.write_glyph_data(x, y, width as u32, height as u32, &rgba_data);

                let glyph_info = GlyphInfo {
                    uv_min: Vec2::new(x as f32 / ATLAS_SIZE as f32, y as f32 / ATLAS_SIZE as f32),
                    uv_max: Vec2::new(
                        (x + width as u32) as f32 / ATLAS_SIZE as f32,
                        (y + height as u32) as f32 / ATLAS_SIZE as f32,
                    ),
                    size: Vec2::new(width as f32 / DPI_SCALE, height as f32 / DPI_SCALE),
                    offset: Vec2::new(
                        image.placement.left as f32 / DPI_SCALE,
                        image.placement.top as f32 / DPI_SCALE,
                    ),
                    advance: 0.0, // Not used in instanced rendering
                };

                let placement = PlacementInfo {
                    left: image.placement.left as f32 / DPI_SCALE,
                    top: image.placement.top as f32 / DPI_SCALE,
                };

                // Cache the result
                self.cache.insert(cache_key, glyph_info);

                Some((glyph_info, placement))
            } else {
                warn!("Atlas full! Cannot pack glyph {}x{}", width, height);
                None
            }
        }
    }
}
