struct VertexInput {
    @builtin(vertex_index) vertex_idx: u32,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) desktop_uv: vec2<f32>,
    @location(1) cursor_uv: vec2<f32>,
};

struct UniformBuffer {
    visible: u32,
    xor_cursor: u32,
    cursor_relative_size: vec2<f32>,
    cursor_pos: vec2<f32>,
};

@group(0) @binding(0)
var t_desktop: texture_2d<f32>;

@group(0) @binding(1)
var s_desktop: sampler;

@group(0) @binding(2)
var t_cursor: texture_2d<f32>;

@group(0) @binding(3)
var s_cursor: sampler;

@group(0) @binding(4)
var<uniform> info: UniformBuffer;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let u = f32(in.vertex_idx & 2u);
    let v = f32((in.vertex_idx << 1u) & 2u);
    let xy = vec2<f32>(u, v) * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0);
    out.clip_pos = vec4<f32>(xy, 0., 1.);
    out.desktop_uv = vec2<f32>(u, v);
    out.cursor_uv = (out.desktop_uv - info.cursor_pos) * info.cursor_relative_size;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let desktop = textureSample(t_desktop, s_desktop, in.desktop_uv);
    let cursor = textureSample(t_cursor, s_cursor, in.cursor_uv);

    // Spec says step() accepts vector, but it doesn't
    let x_guard = step(0., in.cursor_uv.x) - step(1., in.cursor_uv.x);
    let y_guard = step(0., in.cursor_uv.y) - step(1., in.cursor_uv.y);
    let cursor_alpha = x_guard * y_guard * cursor.a;

    return desktop * (1. - cursor_alpha) + cursor * cursor_alpha;
}
