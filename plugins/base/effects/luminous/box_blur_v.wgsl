struct BoxBlurParams {
    radius: i32,
    new_width: i32,
    new_height: i32,
    fixed_size: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;
@group(0) @binding(2) var linear_sampler: sampler;

@group(1) @binding(0) var<storage, read> params_array: BoxBlurParams;

// box_blur_h.wgsl参照。水平パス(プリマルチプライド)を受けて、同じく
// 一律重みのボックスぼかしを縦方向にかけ、最後にun-premultiplyする。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let in_dims = vec2<i32>(textureDimensions(tex));
    let out_coord = vec2<i32>(global_id.xy);

    let radius = params_array.radius;
    let out_dims = vec2<i32>(params_array.new_width, params_array.new_height);

    if (out_coord.x >= out_dims.x || out_coord.y >= out_dims.y) {
        return;
    }

    // サイズ固定時はオフセットなし、通常は出力が上下 radius 分高い
    let y_offset = select(radius, 0, params_array.fixed_size != 0);
    let in_coord = out_coord - vec2<i32>(0, y_offset);

    // 入力は水平パスからのプリマルチプライドアルファ
    // そのまま蓄積してから最後に un-premultiply する
    var color = vec4<f32>(0.0);
    var weight_sum = 0.0;

    for (var dy = -radius; dy <= radius; dy++) {
        let weight = 1.0;
        weight_sum += weight;
        let raw_coord = in_coord + vec2<i32>(0, dy);
        if (params_array.fixed_size != 0) {
            // サイズ固定時はエッジピクセルをクランプして透明化を防ぐ
            let sc = clamp(raw_coord, vec2<i32>(0, 0), in_dims - vec2<i32>(1, 1));
            color += textureLoad(tex, sc, 0) * weight;
        } else if (raw_coord.x >= 0 && raw_coord.x < in_dims.x &&
                   raw_coord.y >= 0 && raw_coord.y < in_dims.y) {
            color += textureLoad(tex, raw_coord, 0) * weight;
        }
        // 境界外(サイズ非固定時)は vec4(0) 扱い → エッジが透明にフェード
    }

    let premul = color / weight_sum;
    var out_color: vec4<f32>;
    if (premul.a > 1e-6) {
        let unpremul = premul.rgb / premul.a;
        out_color = vec4(max(unpremul, vec3<f32>(0.0)), premul.a);
    } else {
        out_color = vec4(0.0);
    }

    textureStore(outputTex, out_coord, out_color);
}
