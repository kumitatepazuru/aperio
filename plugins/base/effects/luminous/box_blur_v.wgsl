struct BoxBlurParams {
    radius: i32,
    out_width: i32,
    out_height: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params_array: BoxBlurParams;

// box_blur_h.wgsl の垂直パス。入力は水平パスからのプリマルチプライドアルファ。
// 一律重みのボックス平均を縦方向にかけ、最後に un-premultiply して戻す。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let in_dims = vec2<i32>(textureDimensions(tex));
    let out_coord = vec2<i32>(global_id.xy);
    let radius = params_array.radius;

    if (out_coord.x >= params_array.out_width || out_coord.y >= params_array.out_height) {
        return;
    }

    var color = vec4<f32>(0.0);
    for (var dy = -radius; dy <= radius; dy++) {
        let sc = clamp(out_coord + vec2<i32>(0, dy), vec2<i32>(0, 0), in_dims - vec2<i32>(1, 1));
        color += textureLoad(tex, sc, 0);
    }

    let premul = color / f32(2 * radius + 1);
    var out_color: vec4<f32>;
    if (premul.a > 1e-6) {
        out_color = vec4<f32>(max(premul.rgb / premul.a, vec3<f32>(0.0)), premul.a);
    } else {
        out_color = vec4<f32>(0.0);
    }

    textureStore(outputTex, out_coord, out_color);
}
