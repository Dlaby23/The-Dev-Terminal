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

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    // Convert pixel coordinates to clip space (-1 to 1)
    // Assuming a viewport of 800x600 for now, will be made configurable
    output.clip_position = vec4<f32>(
        (input.position.x / 400.0) - 1.0,
        1.0 - (input.position.y / 300.0),
        0.0,
        1.0
    );
    output.tex_coords = input.tex_coords;
    output.color = input.color;
    return output;
}

@group(0) @binding(0)
var atlas_texture: texture_2d<f32>;
@group(0) @binding(1)
var atlas_sampler: sampler;

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = textureSample(atlas_texture, atlas_sampler, input.tex_coords).r;
    return vec4<f32>(input.color.rgb, input.color.a * alpha);
}