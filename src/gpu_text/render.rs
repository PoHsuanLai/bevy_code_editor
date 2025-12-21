//! GPU Text Rendering - Instanced glyph rendering using custom shaders
//!
//! This module implements high-performance text rendering using:
//! - Instanced rendering (one draw call for all visible glyphs)
//! - Glyph atlas texture caching
//! - Custom WGSL shader for text with color support

use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d};
use bevy::mesh::{Mesh2d, Indices, PrimitiveTopology};
use bevy::shader::ShaderRef;
use bevy::asset::RenderAssetUsages;

use super::atlas::{GlyphAtlas, GlyphKey};

/// A single glyph instance for GPU rendering
#[derive(Clone, Copy, Debug, Default)]
pub struct GlyphInstance {
    /// Position in screen space (top-left corner)
    pub position: Vec2,
    /// UV coordinates in atlas (min corner)
    pub uv_min: Vec2,
    /// UV coordinates in atlas (max corner)
    pub uv_max: Vec2,
    /// Size of the glyph in pixels
    pub size: Vec2,
    /// Color (RGBA)
    pub color: Color,
}

/// A batch of glyph instances to render
#[derive(Component, Default)]
pub struct GlyphBatch {
    /// All glyph instances in this batch
    pub instances: Vec<GlyphInstance>,
    /// Whether the batch needs to be re-uploaded to GPU
    pub dirty: bool,
}

impl GlyphBatch {
    pub fn new() -> Self {
        Self {
            instances: Vec::with_capacity(4096),
            dirty: true,
        }
    }

    pub fn clear(&mut self) {
        self.instances.clear();
        self.dirty = true;
    }

    pub fn push(&mut self, instance: GlyphInstance) {
        self.instances.push(instance);
        self.dirty = true;
    }

    /// Add a character to the batch
    pub fn add_char(
        &mut self,
        character: char,
        x: f32,
        y: f32,
        font_size: f32,
        color: Color,
        atlas: &mut GlyphAtlas,
    ) -> f32 {
        let key = GlyphKey::new(character, font_size);

        // Get or rasterize the glyph
        let glyph_info = atlas.get_or_insert(key, || {
            super::atlas::GlyphRasterizer::rasterize(character, font_size)
        });

        if let Some(info) = glyph_info {
            self.push(GlyphInstance {
                position: Vec2::new(x + info.offset.x, y - info.offset.y),
                uv_min: info.uv_min,
                uv_max: info.uv_max,
                size: info.size,
                color,
            });
            info.advance
        } else {
            // Fallback advance for missing glyphs
            font_size * 0.6
        }
    }

    /// Add a string to the batch
    pub fn add_string(
        &mut self,
        text: &str,
        mut x: f32,
        y: f32,
        font_size: f32,
        color: Color,
        atlas: &mut GlyphAtlas,
    ) -> f32 {
        let start_x = x;
        for ch in text.chars() {
            if ch == '\n' || ch == '\r' {
                continue;
            }
            if ch == '\t' {
                // Tab = 4 spaces
                x += font_size * 0.6 * 4.0;
                continue;
            }
            x += self.add_char(ch, x, y, font_size, color, atlas);
        }
        x - start_x
    }
}

/// Custom material for GPU text rendering
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct TextMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub atlas_texture: Handle<Image>,

    /// Base color multiplier
    #[uniform(2)]
    pub color: LinearRgba,
}

impl Material2d for TextMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/text_glyph.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

/// Resource to track the current text material
#[derive(Resource)]
pub struct TextRenderState {
    pub material_handle: Option<Handle<TextMaterial>>,
    pub mesh_handle: Option<Handle<Mesh>>,
}

impl Default for TextRenderState {
    fn default() -> Self {
        Self {
            material_handle: None,
            mesh_handle: None,
        }
    }
}

/// Create a quad mesh for rendering glyphs
pub fn create_quad_mesh() -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());

    // Vertices for a unit quad (0,0) to (1,1)
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [0.0, 0.0, 0.0], // bottom-left
            [1.0, 0.0, 0.0], // bottom-right
            [1.0, 1.0, 0.0], // top-right
            [0.0, 1.0, 0.0], // top-left
        ],
    );

    // UV coordinates
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vec![
            [0.0, 1.0], // bottom-left (flipped Y for texture)
            [1.0, 1.0], // bottom-right
            [1.0, 0.0], // top-right
            [0.0, 0.0], // top-left
        ],
    );

    // Normals (facing camera)
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        vec![
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ],
    );

    // Indices for two triangles
    mesh.insert_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));

    mesh
}

/// Marker component for GPU-rendered text entities
#[derive(Component)]
pub struct GpuTextGlyph {
    pub line_index: usize,
    pub char_index: usize,
}

/// Plugin for GPU text rendering
pub struct GpuTextPlugin;

impl Plugin for GpuTextPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Material2dPlugin::<TextMaterial>::default())
            .init_resource::<TextRenderState>()
            .add_systems(Startup, setup_gpu_text)
            .add_systems(Update, update_atlas_texture);
    }
}

fn setup_gpu_text(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TextMaterial>>,
    mut render_state: ResMut<TextRenderState>,
) {
    // Create the glyph atlas
    let atlas = GlyphAtlas::new(&mut images);

    // Create the text material
    let material = TextMaterial {
        atlas_texture: atlas.texture.clone(),
        color: LinearRgba::WHITE,
    };
    render_state.material_handle = Some(materials.add(material));

    // Create the quad mesh
    render_state.mesh_handle = Some(meshes.add(create_quad_mesh()));

    commands.insert_resource(atlas);
}

fn update_atlas_texture(
    mut atlas: ResMut<GlyphAtlas>,
    mut images: ResMut<Assets<Image>>,
) {
    atlas.update_texture(&mut images);
}

/// Spawns glyph entities for a line of text using GPU rendering
pub fn spawn_gpu_text_line(
    commands: &mut Commands,
    line_index: usize,
    x: f32,
    y: f32,
    segments: &[(String, Color)],
    font_size: f32,
    char_width: f32,
    atlas: &mut GlyphAtlas,
    render_state: &TextRenderState,
) -> Vec<Entity> {
    let mut entities = Vec::new();
    let mut current_x = x;
    let mut char_index = 0;

    let Some(mesh_handle) = &render_state.mesh_handle else {
        return entities;
    };
    let Some(material_handle) = &render_state.material_handle else {
        return entities;
    };

    for (text, _color) in segments {
        for ch in text.chars() {
            if ch == '\n' || ch == '\r' {
                continue;
            }

            let advance = if ch == '\t' {
                char_width * 4.0
            } else {
                let key = GlyphKey::new(ch, font_size);
                if let Some(info) = atlas.get_or_insert(key, || {
                    super::atlas::GlyphRasterizer::rasterize(ch, font_size)
                }) {
                    // Spawn a glyph entity
                    let entity = commands.spawn((
                        Mesh2d(mesh_handle.clone()),
                        MeshMaterial2d(material_handle.clone()),
                        Transform::from_translation(Vec3::new(
                            current_x + info.offset.x,
                            y - info.offset.y,
                            0.0,
                        ))
                        .with_scale(Vec3::new(info.size.x, info.size.y, 1.0)),
                        GpuTextGlyph {
                            line_index,
                            char_index,
                        },
                    )).id();

                    entities.push(entity);
                    info.advance
                } else {
                    char_width
                }
            };

            current_x += advance;
            char_index += 1;
        }
    }

    entities
}

/// Builder for creating text render batches
pub struct TextBatchBuilder<'a> {
    batch: &'a mut GlyphBatch,
    atlas: &'a mut GlyphAtlas,
    font_size: f32,
    #[allow(dead_code)]
    line_height: f32,
    char_width: f32,
}

impl<'a> TextBatchBuilder<'a> {
    pub fn new(
        batch: &'a mut GlyphBatch,
        atlas: &'a mut GlyphAtlas,
        font_size: f32,
        line_height: f32,
    ) -> Self {
        Self {
            batch,
            atlas,
            font_size,
            line_height,
            char_width: font_size * 0.6,
        }
    }

    /// Add a line of text with syntax highlighting segments
    pub fn add_line(
        &mut self,
        y: f32,
        start_x: f32,
        segments: &[(String, Color)],
    ) {
        let mut x = start_x;
        for (text, color) in segments {
            for ch in text.chars() {
                if ch == '\t' {
                    x += self.char_width * 4.0;
                    continue;
                }
                if ch == '\n' || ch == '\r' {
                    continue;
                }

                let key = GlyphKey::new(ch, self.font_size);
                if let Some(info) = self.atlas.get_or_insert(key, || {
                    super::atlas::GlyphRasterizer::rasterize(ch, self.font_size)
                }) {
                    self.batch.push(GlyphInstance {
                        position: Vec2::new(x + info.offset.x, y - info.offset.y),
                        uv_min: info.uv_min,
                        uv_max: info.uv_max,
                        size: info.size,
                        color: *color,
                    });
                    x += info.advance;
                } else {
                    x += self.char_width;
                }
            }
        }
    }

    /// Add a simple line of text (single color)
    pub fn add_simple_line(&mut self, y: f32, start_x: f32, text: &str, color: Color) {
        self.add_line(y, start_x, &[(text.to_string(), color)]);
    }
}
