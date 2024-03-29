struct VertexInput {
    @builtin(vertex_index) vertex_idx: u32,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0)
var t_main: texture_2d<f32>;

@group(0) @binding(1)
var s_main: sampler;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let u = f32(in.vertex_idx & 2u);
    let v = f32((in.vertex_idx << 1u) & 2u);
    let xy = vec2<f32>(u, v) * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0);
    out.clip_pos = vec4<f32>(xy, 0., 1.);
    out.uv = vec2<f32>(u, v);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_main, s_main, in.uv);
}
