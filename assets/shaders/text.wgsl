// GPU Text Rendering Shader for bevy_code_editor
// Inspired by Zed's GPUI text rendering approach
//
// This shader renders text glyphs using instanced rendering.
// Each glyph is a quad that samples from a glyph atlas texture.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput

// Uniforms
@group(2) @binding(0) var<uniform> viewport_size: vec2<f32>;
@group(2) @binding(1) var<uniform> scroll_offset: vec2<f32>;

// Atlas texture
@group(2) @binding(2) var atlas_texture: texture_2d<f32>;
@group(2) @binding(3) var atlas_sampler: sampler;

// Per-instance data (passed via vertex attributes in Bevy)
struct GlyphInstance {
    // Position in screen space
    @location(0) position: vec2<f32>,
    // UV coordinates in atlas (min)
    @location(1) uv_min: vec2<f32>,
    // UV coordinates in atlas (max)
    @location(2) uv_max: vec2<f32>,
    // Size of the glyph in pixels
    @location(3) size: vec2<f32>,
    // Color (RGBA)
    @location(4) color: vec4<f32>,
}

struct VertexInput {
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
}

struct FragmentInput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

// Vertex shader - generates quad vertices for each glyph instance
@vertex
fn vertex(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) position: vec2<f32>,
    @location(1) uv_min: vec2<f32>,
    @location(2) uv_max: vec2<f32>,
    @location(3) size: vec2<f32>,
    @location(4) color: vec4<f32>,
) -> FragmentInput {
    // Generate quad vertices (triangle strip: 0,1,2,3)
    // 0 -- 1
    // |    |
    // 2 -- 3
    let unit_x = f32(vertex_index & 1u);
    let unit_y = f32((vertex_index >> 1u) & 1u);
    let unit_vertex = vec2<f32>(unit_x, unit_y);

    // Calculate screen position
    let screen_pos = position + unit_vertex * size - scroll_offset;

    // Convert to clip space (-1 to 1)
    let clip_pos = (screen_pos / viewport_size) * 2.0 - 1.0;
    // Flip Y for Bevy's coordinate system
    let final_pos = vec4<f32>(clip_pos.x, -clip_pos.y, 0.0, 1.0);

    // Interpolate UV coordinates
    let uv = mix(uv_min, uv_max, unit_vertex);

    var out: FragmentInput;
    out.position = final_pos;
    out.uv = uv;
    out.color = color;
    return out;
}

// Fragment shader - samples glyph from atlas and applies color
@fragment
fn fragment(in: FragmentInput) -> @location(0) vec4<f32> {
    // Sample the glyph alpha from the atlas
    let atlas_sample = textureSample(atlas_texture, atlas_sampler, in.uv);

    // Use the atlas alpha with the instance color
    let alpha = atlas_sample.a * in.color.a;

    // Discard fully transparent pixels for performance
    if alpha < 0.01 {
        discard;
    }

    // Premultiplied alpha output
    return vec4<f32>(in.color.rgb * alpha, alpha);
}
