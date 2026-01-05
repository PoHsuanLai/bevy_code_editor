// GPU Text Rendering Shader for bevy_code_editor
// Inspired by Zed's GPUI text rendering approach
//
// This shader renders text glyphs using instanced rendering.
// Each glyph is a quad that samples from a glyph atlas texture.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput

// Uniforms
struct TextGlobals {
    viewport_size: vec2<f32>,
    scroll_offset: vec2<f32>,
}

@group(0) @binding(0) var<uniform> globals: TextGlobals;

// Atlas texture
@group(1) @binding(0) var atlas_texture: texture_2d<f32>;
@group(1) @binding(1) var atlas_sampler: sampler;

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
    // position is in Bevy world coordinates (center-origin)
    // globals.scroll_offset is currently applied on the CPU side during layout,
    // so we don't need to apply it here unless we change the CPU logic.
    let screen_pos = position + unit_vertex * size;

    // Convert to clip space (-1 to 1)
    // Bevy World (0,0) is center.
    // Clip (0,0) is center.
    // Just divide by half-size.
    let half_size = globals.viewport_size * 0.5;
    let clip_pos = screen_pos / half_size;

    let final_pos = vec4<f32>(clip_pos.x, clip_pos.y, 0.0, 1.0);

    // Interpolate UV coordinates
    // Flip Y for texture space (0=top, 1=bottom) vs geometry space (0=bottom, 1=top)
    let uv = mix(uv_min, uv_max, vec2<f32>(unit_vertex.x, 1.0 - unit_vertex.y));

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
