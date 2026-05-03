// Converts 2-plane semi-planar YUV (NV12: Y + interleaved UV) to Rgba16Float.
// Y  → R8Unorm / R16Unorm  texture (full luma size)
// UV → Rg8Unorm / Rg16Unorm texture (chroma half-size; R=Cb, G=Cr)

struct YuvParams {
    y_offset: f32,
    y_scale:  f32,
    c_offset: f32,
    c_scale:  f32,
}

@group(0) @binding(0) var y_tex:      texture_2d<f32>;
@group(0) @binding(1) var uv_tex:     texture_2d<f32>;
@group(0) @binding(2) var yuv_sampler: sampler;
@group(0) @binding(3) var<uniform>    params: YuvParams;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0)       uv:       vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 3.0,  1.0),
        vec2<f32>(-1.0, -3.0),
    );
    var uvs = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(2.0, 0.0),
        vec2<f32>(0.0, 2.0),
    );
    var out: VertexOutput;
    out.position = vec4<f32>(pos[idx], 0.0, 1.0);
    out.uv       = uvs[idx];
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let y_raw  = textureSample(y_tex,  yuv_sampler, in.uv).r;
    let uv_raw = textureSample(uv_tex, yuv_sampler, in.uv).rg;

    let y  = (y_raw    - params.y_offset) * params.y_scale;
    let cb = (uv_raw.r - params.c_offset) * params.c_scale;
    let cr = (uv_raw.g - params.c_offset) * params.c_scale;

    let r = clamp(y + 1.5748 * cr,                 0.0, 1.0);
    let g = clamp(y - 0.18732 * cb - 0.46812 * cr, 0.0, 1.0);
    let b = clamp(y + 1.8556 * cb,                 0.0, 1.0);

    return vec4<f32>(r, g, b, 1.0);
}
