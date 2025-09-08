struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> screen_size: vec2<f32>;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    // Convert pixel coordinates to clip space
    output.clip_position = vec4<f32>(
        (input.position.x / screen_size.x) * 2.0 - 1.0,
        1.0 - (input.position.y / screen_size.y) * 2.0,
        0.0,
        1.0
    );
    output.tex_coords = input.tex_coords;
    output.color = input.color;
    return output;
}

@group(0) @binding(1)
var glyph_texture: texture_2d<f32>;
@group(0) @binding(2)
var glyph_sampler: sampler;

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // For now, just return the color directly (colored blocks)
    // In a real implementation, we'd sample the glyph texture
    return input.color;
}