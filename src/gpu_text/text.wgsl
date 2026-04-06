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

struct FragmentInput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    // Local position within the quad (0..size) for rounded corner SDF
    @location(2) local_pos: vec2<f32>,
    @location(3) quad_size: vec2<f32>,
    @location(4) corner_radius: f32,
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
    @location(5) z_index: f32,
    @location(6) corner_radius: f32,
    @location(7) skew: f32,
) -> FragmentInput {
    // Generate quad vertices (triangle list: 0,1,2, 3,4,5)
    // 2 (0,1) -- 5 (1,1)
    // |         / |
    // |       /   |
    // |     /     |
    // 0 (0,0) --- 1/4 (1,0)

    var vertices = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), // BL (v0)
        vec2<f32>(1.0, 0.0), // BR (v1)
        vec2<f32>(0.0, 1.0), // TL (v2)
        vec2<f32>(0.0, 1.0), // TL (v3)
        vec2<f32>(1.0, 0.0), // BR (v4)
        vec2<f32>(1.0, 1.0)  // TR (v5)
    );

    let unit_vertex = vertices[vertex_index];

    // Calculate screen position with optional italic skew
    // Skew shifts x proportionally to y (top of glyph shifts right)
    var screen_pos = position + unit_vertex * size;
    screen_pos.x += skew * unit_vertex.y * size.y;

    // Convert to clip space
    let half_size = globals.viewport_size * 0.5;
    let clip_pos = screen_pos / half_size;

    // Normalize z_index to clip space (-1 to 1) - higher z_index renders on top
    // Map z_index range [0, 1000] to clip space [0.0, 1.0] for depth testing
    let normalized_z = z_index / 1000.0;
    let final_pos = vec4<f32>(clip_pos.x, clip_pos.y, normalized_z, 1.0);

    // Interpolate UV coordinates
    // Flip Y for texture space (0=top, 1=bottom) vs geometry space (0=bottom, 1=top)
    let uv = mix(uv_min, uv_max, vec2<f32>(unit_vertex.x, 1.0 - unit_vertex.y));

    var out: FragmentInput;
    out.position = final_pos;
    out.uv = uv;
    out.color = color;
    out.local_pos = unit_vertex * size;
    out.quad_size = size;
    out.corner_radius = corner_radius;
    return out;
}

// Signed distance function for a rounded rectangle
fn rounded_rect_sdf(pos: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let q = abs(pos) - half_size + vec2<f32>(radius);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - radius;
}

// Fragment shader - samples glyph from atlas and applies color
@fragment
fn fragment(in: FragmentInput) -> @location(0) vec4<f32> {
    // Rounded corner clipping
    if in.corner_radius > 0.0 {
        let center = in.quad_size * 0.5;
        let pos = in.local_pos - center;
        let d = rounded_rect_sdf(pos, center, in.corner_radius);
        if d > 0.5 {
            discard;
        }
        // Anti-alias the edge
        let edge_alpha = 1.0 - smoothstep(-0.5, 0.5, d);

        // Sample the glyph alpha from the atlas
        let atlas_sample = textureSample(atlas_texture, atlas_sampler, in.uv);
        let alpha = atlas_sample.a * in.color.a * edge_alpha;

        if alpha < 0.01 {
            discard;
        }
        return vec4<f32>(in.color.rgb * alpha, alpha);
    }

    // Standard path (no rounded corners) — unchanged
    let atlas_sample = textureSample(atlas_texture, atlas_sampler, in.uv);
    let alpha = atlas_sample.a * in.color.a;

    if alpha < 0.01 {
        discard;
    }

    return vec4<f32>(in.color.rgb * alpha, alpha);
}
