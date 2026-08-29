enable wgpu_binding_array;

struct MirrorParams {
    axis: i32,
    orig_start: i32,
    dim: i32,
    mirror_start: i32,
    mirror_len: i32,
    mirror_a: i32,
    dist_c: i32,
    dist_slope: i32,
    base_opacity: f32,
    falloff_coef: f32,
    out_width: i32,
    out_height: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params_array: MirrorParams;

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let out_coord = vec2<i32>(global_id.xy);
    let out_dims = vec2<i32>(params_array.out_width, params_array.out_height);
    if (out_coord.x >= out_dims.x || out_coord.y >= out_dims.y) {
        return;
    }

    let vertical = params_array.axis == 0;
    let t = select(out_coord.x, out_coord.y, vertical);
    let s = select(out_coord.y, out_coord.x, vertical);
    let tex = inputTex[0];

    if (t >= params_array.orig_start && t < params_array.orig_start + params_array.dim) {
        let src_axis = t - params_array.orig_start;
        let src = select(vec2<i32>(src_axis, s), vec2<i32>(s, src_axis), vertical);
        textureStore(outputTex, out_coord, textureLoad(tex, src, 0));
        return;
    }
    if (t >= params_array.mirror_start && t < params_array.mirror_start + params_array.mirror_len) {
        let src_axis = params_array.mirror_a - t;
        let src = select(vec2<i32>(src_axis, s), vec2<i32>(s, src_axis), vertical);
        let dist = f32(params_array.dist_c + params_array.dist_slope * t);
        let opacity = clamp(params_array.base_opacity - params_array.falloff_coef * dist, 0.0, 1.0);
        var color = textureLoad(tex, src, 0);
        color.a = color.a * opacity;
        textureStore(outputTex, out_coord, color);
        return;
    }
    textureStore(outputTex, out_coord, vec4<f32>(0.0, 0.0, 0.0, 0.0));
}
