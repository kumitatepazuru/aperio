struct BlurParams {
    radius: i32,
    new_width: i32,
    new_height: i32
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;
@group(0) @binding(2) var linear_sampler: sampler;

@group(1) @binding(0) var<storage, read> params_array: BlurParams;

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

    // 水平オフセット: 出力は入力より左右それぞれ radius 分広い
    let in_coord = out_coord - vec2<i32>(radius, 0);

    if (radius <= 0) {
        // 垂直パスのためにプリマルチプライドアルファで出力
        let s = textureLoad(tex, in_coord, 0);
        textureStore(outputTex, out_coord, vec4(s.rgb * s.a, s.a));
        return;
    }

    let sigma2 = 2.0 * pow(f32(radius) / 3.0, 2.0);

    var color = vec4<f32>(0.0);
    var weight_sum = 0.0;

    for (var dx = -radius; dx <= radius; dx++) {
        let weight = exp(-f32(dx * dx) / sigma2);
        weight_sum += weight;
        let sample_coord = in_coord + vec2<i32>(dx, 0);
        if (sample_coord.x >= 0 && sample_coord.x < in_dims.x &&
            sample_coord.y >= 0 && sample_coord.y < in_dims.y) {
            let s = textureLoad(tex, sample_coord, 0);
            // プリマルチプライドアルファで蓄積: 透明ピクセルの RGB が混入しない
            color += vec4(s.rgb * s.a, s.a) * weight;
        }
        // 境界外は vec4(0) 扱い、ただし weight_sum には加算 → エッジが透明にフェード
    }

    // プリマルチプライドアルファのまま出力 (垂直パスへの中間テクスチャ)
    textureStore(outputTex, out_coord, color / weight_sum);
}
