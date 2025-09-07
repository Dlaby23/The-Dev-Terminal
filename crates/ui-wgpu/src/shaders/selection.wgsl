struct VsIn {
    @location(0) pos: vec2<f32>,   // pixel-space coordinates
    @location(1) color: vec4<f32>,
}

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> screen: vec2<f32>; // width, height in pixels

@vertex
fn vs_main(in: VsIn) -> VsOut {
    // convert pixel coords (0..W, 0..H) to NDC (-1..1)
    let ndc_x = (in.pos.x / screen.x) * 2.0 - 1.0;
    // flip Y because we use pixel top-left, WGPU expects bottom-left for NDC
    let ndc_y = 1.0 - (in.pos.y / screen.y) * 2.0;
    var out: VsOut;
    out.clip_pos = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return in.color; // premult not required; we set blend to src-alpha
}