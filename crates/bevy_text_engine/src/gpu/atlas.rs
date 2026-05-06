//! Glyph atlas: rasterizes glyphs once via cosmic_text and caches them in a GPU texture.

use bevy::asset::{AssetId, RenderAssetUsages};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::text::Font;
use cosmic_text::{FontSystem, SwashCache};
use std::collections::HashMap;

/// Power of 2 for GPU efficiency.
pub const ATLAS_SIZE: u32 = 2048;

const GLYPH_PADDING: u32 = 2;

/// Rasterize at 2x for crisp text on HiDPI displays.
pub const DPI_SCALE: f32 = 2.0;

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
    rows: Vec<AtlasRow>,
    current_y: u32,
    pixels: Vec<u8>,
    pub dirty: bool,
    font_system: FontSystem,
    swash_cache: SwashCache,
    configured_font_id: Option<cosmic_text::fontdb::ID>,
    /// `bevy_text::Font` handles registered with the cosmic-text fontdb,
    /// keyed by AssetId so re-registration is a no-op on subsequent frames.
    loaded_fonts: HashMap<AssetId<Font>, cosmic_text::fontdb::ID>,
    /// Cache keyed by cosmic_text CacheKey — populated by `get_or_rasterize_glyph`.
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
            rows: Vec::new(),
            current_y: 0,
            pixels,
            dirty: false,
            font_system,
            swash_cache,
            configured_font_id,
            loaded_fonts: HashMap::new(),
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

    /// Register a `bevy_text::Font` asset's bytes into the cosmic-text font
    /// system on first use; subsequent calls are O(1) cache hits. Returns
    /// the `fontdb::ID` to feed into `shape_line`.
    pub fn ensure_font(
        &mut self,
        handle: &Handle<Font>,
        fonts: &Assets<Font>,
    ) -> Option<cosmic_text::fontdb::ID> {
        let id = handle.id();
        if let Some(font_id) = self.loaded_fonts.get(&id) {
            return Some(*font_id);
        }
        let font = fonts.get(handle)?;
        let bytes: Vec<u8> = (*font.data).clone();
        let db = self.font_system.db_mut();
        let count_before = db.faces().count();
        db.load_font_data(bytes);
        let font_id = db.faces().nth(count_before).map(|f| f.id)?;
        self.loaded_fonts.insert(id, font_id);
        Some(font_id)
    }

    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        if width == 0 || height == 0 {
            return Some((0, 0));
        }

        let padded_width = width + GLYPH_PADDING;
        let padded_height = height + GLYPH_PADDING;

        for row in &mut self.rows {
            if row.height >= padded_height && row.x_cursor + padded_width <= ATLAS_SIZE {
                let x = row.x_cursor;
                let y = row.y;
                row.x_cursor += padded_width;
                return Some((x, y));
            }
        }

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

        None
    }

    /// Partial upload: only the dirty row range.
    pub fn update_texture(&mut self, images: &mut Assets<Image>) {
        if !self.dirty || self.dirty_min_y >= self.dirty_max_y {
            self.dirty = false;
            return;
        }

        let min_y = self.dirty_min_y.min(ATLAS_SIZE);
        let max_y = self.dirty_max_y.min(ATLAS_SIZE);

        if let Some(image) = images.get_mut(&self.texture) {
            if let Some(ref mut data) = image.data {
                let row_bytes = (ATLAS_SIZE * 4) as usize;
                let start_byte = min_y as usize * row_bytes;
                let end_byte = max_y as usize * row_bytes;

                if end_byte <= data.len() && end_byte <= self.pixels.len() {
                    data[start_byte..end_byte].copy_from_slice(&self.pixels[start_byte..end_byte]);
                } else {
                    data.copy_from_slice(&self.pixels);
                }
            }
        } else {
            // No existing image — create fresh (first frame).
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

        self.dirty = false;
        self.dirty_min_y = ATLAS_SIZE;
        self.dirty_max_y = 0;
    }

    /// Clear the atlas when font changes or atlas is full.
    pub fn clear(&mut self) {
        self.rows.clear();
        self.current_y = 0;
        self.pixels.fill(0);
        self.dirty = true;
        self.cache.clear();
        self.generation += 1;
        self.dirty_min_y = 0;
        self.dirty_max_y = ATLAS_SIZE;
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

    /// Pre-rasterize a batch of `cosmic_text::CacheKey`s into the atlas,
    /// ignoring the result. Used by `display_map` to warm the atlas before
    /// the renderer runs, so the renderer's paint pass never triggers
    /// mid-frame texture uploads. Cache hits are O(1) and skip the work.
    pub fn ensure_glyphs<I: IntoIterator<Item = cosmic_text::CacheKey>>(&mut self, keys: I) {
        for key in keys {
            if self.cache.contains_key(&key) {
                continue;
            }
            // Drop the result; we just need the side effect of insertion.
            let _ = self.get_or_rasterize_glyph(key);
        }
    }
}

/// Inserts [`GlyphAtlas`] at startup.
pub struct GlyphAtlasPlugin;

impl Plugin for GlyphAtlasPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_glyph_atlas);
    }
}

fn setup_glyph_atlas(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(GlyphAtlas::new(&mut images));
}

pub use instanced_extensions::*;

mod instanced_extensions {
    use super::*;
    use cosmic_text::{Attrs, AttrsList, ShapeBuffer, ShapeLine, Shaping};

    #[derive(Clone, Copy, Debug)]
    pub struct PlacementInfo {
        pub left: f32,
        pub top: f32,
    }

    impl GlyphAtlas {
        pub(crate) fn pack(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
            self.allocate(width, height)
        }

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
                        self.pixels[dst_idx] = data[src_idx];
                        self.pixels[dst_idx + 1] = data[src_idx + 1];
                        self.pixels[dst_idx + 2] = data[src_idx + 2];
                        self.pixels[dst_idx + 3] = data[src_idx + 3];
                    }
                }
            }

            self.dirty_min_y = self.dirty_min_y.min(y);
            self.dirty_max_y = self.dirty_max_y.max(y + height);
            self.dirty = true;
        }

        /// Shape a line into the engine's owned `LineShape`. Pass a
        /// `fontdb::ID` to pin shaping to a specific face (e.g. one
        /// returned by [`GlyphAtlas::ensure_font`]); pass `None` to use
        /// the constructor's `font_path` font, falling back to system fonts.
        pub fn shape_line(
            &mut self,
            text: &str,
            font_size: f32,
            font_id: Option<cosmic_text::fontdb::ID>,
        ) -> crate::view::snapshot::LineShape {
            use crate::view::snapshot::{LineShape, ShapedGlyph};

            let pinned = font_id.or(self.configured_font_id);
            let mut attrs = Attrs::new();
            let pinned_family = pinned.and_then(|id| {
                self.font_system
                    .db()
                    .face(id)
                    .and_then(|f| f.families.first().map(|fam| fam.0.clone()))
            });
            if let Some(ref family) = pinned_family {
                attrs = attrs.family(cosmic_text::Family::Name(family.as_str()));
            }
            let attrs_list = AttrsList::new(attrs);

            let line = ShapeLine::new(
                &mut self.font_system,
                text,
                &attrs_list,
                Shaping::Advanced,
                4,
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
                return LineShape {
                    glyphs: Vec::new(),
                    width: 0.0,
                    font_size,
                };
            }

            let layout = &layout_lines[0];
            let mut glyphs = Vec::with_capacity(layout.glyphs.len());
            for g in &layout.glyphs {
                let physical = g.physical((0.0, 0.0), DPI_SCALE);
                glyphs.push(ShapedGlyph {
                    x: g.x,
                    byte_index: g.start,
                    cache_key: physical.cache_key,
                });
            }

            LineShape {
                glyphs,
                width: layout.w,
                font_size,
            }
        }

        pub fn get_or_rasterize_glyph(
            &mut self,
            cache_key: cosmic_text::CacheKey,
        ) -> Option<(GlyphInfo, PlacementInfo)> {
            use swash::scale::image::Content;

            // Check cache first. `PlacementInfo` is reconstructed from
            // `GlyphInfo.offset` (which already stores left/top in logical
            // pixels), so we don't need to cache it separately.
            if let Some(info) = self.cache.get(&cache_key) {
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
                    advance: 0.0,
                };

                let placement = PlacementInfo {
                    left: image.placement.left as f32 / DPI_SCALE,
                    top: image.placement.top as f32 / DPI_SCALE,
                };

                self.cache.insert(cache_key, glyph_info);

                Some((glyph_info, placement))
            } else {
                warn!("Atlas full! Cannot pack glyph {}x{}", width, height);
                None
            }
        }
    }
}
