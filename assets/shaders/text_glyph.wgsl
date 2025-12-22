// GPU Text Rendering Shader for bevy_code_editor
// Using Bevy's Material2d system with glyph atlas

#import bevy_sprite::mesh2d_vertex_output::VertexOutput

// Material bindings - matches TextMaterial struct
@group(2) @binding(0) var atlas_texture: texture_2d<f32>;
@group(2) @binding(1) var atlas_sampler: sampler;
@group(2) @binding(2) var<uniform> base_color: vec4<f32>;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    // Sample the glyph from the atlas texture
    let atlas_sample = textureSample(atlas_texture, atlas_sampler, mesh.uv);

    // Use vertex color for syntax highlighting
    let text_color = mesh.color;

    // The atlas stores white glyphs with alpha channel
    // Multiply by text color to get the final colored glyph
    let alpha = atlas_sample.a * text_color.a;

    // Discard fully transparent pixels
    if alpha < 0.01 {
        discard;
    }

    // Output the colored text
    return vec4<f32>(text_color.rgb, alpha);
}
