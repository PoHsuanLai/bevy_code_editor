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
    // Per-corner radii: [top_left, top_right, bottom_left, bottom_right].
    // The fragment SDF picks the matching radius for the corner the
    // sampled point is nearest, allowing asymmetric rounding (e.g. only
    // the top corners on the first row of a multi-row block panel).
    @location(4) corner_radii: vec4<f32>,
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
    @location(5) corner_radii: vec4<f32>,
    @location(6) z_index: f32,
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
    out.corner_radii = corner_radii;
    return out;
}

// SDF for a rounded rectangle with per-corner radii. `pos` is centered
// (origin at quad center). `half_size` is half the quad's extent. The
// radius used is selected by which quadrant `pos` is in:
//   - top-left  (x<0, y>0): radii.x
//   - top-right (x>0, y>0): radii.y
//   - bot-left  (x<0, y<0): radii.z
//   - bot-right (x>0, y<0): radii.w
//
// In the geometry-space coordinate system the vertex shader uses
// `unit_vertex.y` increases bottom→top, so positive local_pos.y is the
// top of the quad — matches the [tl, tr, bl, br] mapping in `radii`.
fn rounded_rect_sdf_per_corner(
    pos: vec2<f32>,
    half_size: vec2<f32>,
    radii: vec4<f32>,
) -> f32 {
    // Pick this fragment's corner radius based on its quadrant.
    let r = select(
        select(radii.z, radii.w, pos.x > 0.0),
        select(radii.x, radii.y, pos.x > 0.0),
        pos.y > 0.0,
    );
    let q = abs(pos) - half_size + vec2<f32>(r);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - r;
}

// Fragment shader - samples glyph from atlas and applies color
@fragment
fn fragment(in: FragmentInput) -> @location(0) vec4<f32> {
    // Rounded corner clipping when any corner has a non-zero radius.
    // Per-corner: a rect with `[r, r, 0, 0]` rounds only the top corners
    // (used by the first row of a multi-row block panel).
    let max_radius = max(
        max(in.corner_radii.x, in.corner_radii.y),
        max(in.corner_radii.z, in.corner_radii.w),
    );
    if max_radius > 0.0 {
        let center = in.quad_size * 0.5;
        let pos = in.local_pos - center;
        let d = rounded_rect_sdf_per_corner(pos, center, in.corner_radii);
        if d > 0.5 {
            discard;
        }
        // Anti-alias the edge — but only when the fragment is actually
        // inside a rounded corner's curve. A rect with `[r, r, 0, 0]`
        // (rounded top, sharp bottom) used to AA-fade its sharp bottom
        // edge too, since the SDF returns d=0 there and smoothstep
        // produced ~0.5 alpha. That fade left a half-transparent strip
        // between adjacent rows in a multi-row panel.
        //
        // Inside a rounded-corner quadrant the SDF is curve-driven; on
        // a sharp edge the SDF degenerates to a straight `q.x` or
        // `q.y`. We detect this geometrically: pick the corner radius
        // for the fragment's quadrant, and only apply AA when the
        // fragment sits inside that corner's bounding square (where
        // both `q.x > 0` and `q.y > 0` — the curved region).
        let r = pick_corner_radius(pos, in.corner_radii);
        let q = abs(pos) - center + vec2<f32>(r);
        let in_curve = r > 0.0 && q.x > 0.0 && q.y > 0.0;
        let edge_alpha = select(1.0, 1.0 - smoothstep(-0.5, 0.5, d), in_curve);

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

// Pick the corner radius for a fragment's quadrant. Mirrors
// `rounded_rect_sdf_per_corner`'s quadrant selection — kept in sync
// so the AA gate uses the same radius the SDF uses.
fn pick_corner_radius(pos: vec2<f32>, radii: vec4<f32>) -> f32 {
    return select(
        select(radii.z, radii.w, pos.x > 0.0),
        select(radii.x, radii.y, pos.x > 0.0),
        pos.y > 0.0,
    );
}
