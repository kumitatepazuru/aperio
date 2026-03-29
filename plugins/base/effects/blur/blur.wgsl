struct BlurParams {
    radius: i32
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

    // 出力サイズ = 入力サイズ + 2*radius (各辺 radius 分拡張)
    let radius = params_array.radius;
    let out_dims = in_dims + vec2<i32>(radius * 2);

    if (out_coord.x >= out_dims.x || out_coord.y >= out_dims.y) {
        return;
    }

    // 出力座標を入力座標空間に変換 (入力は出力の中央に配置)
    let in_coord = out_coord - vec2<i32>(radius);

    if (radius <= 0) {
        textureStore(outputTex, out_coord, textureLoad(tex, in_coord, 0));
        return;
    }

    // ガウシアンぼかし (sigma = radius / 3.0)
    // 入力範囲外のピクセルは透明 (vec4(0.0)) として扱い、
    // 重みは常に加算することで境界付近のアルファを滑らかにフェードさせる
    let sigma2 = 2.0 * pow(f32(radius) / 3.0, 2.0);

    var color = vec4<f32>(0.0);
    var weight_sum = 0.0;

    for (var dy = -radius; dy <= radius; dy++) {
        for (var dx = -radius; dx <= radius; dx++) {
            let weight = exp(-f32(dx * dx + dy * dy) / sigma2);
            let sample_coord = in_coord + vec2<i32>(dx, dy);
            if (sample_coord.x >= 0 && sample_coord.x < in_dims.x &&
                sample_coord.y >= 0 && sample_coord.y < in_dims.y) {
                color += textureLoad(tex, sample_coord, 0) * weight;
            }
            // 範囲外は vec4(0.0) 扱いだが重みは加算 → 境界が透明にフェード
            weight_sum += weight;
        }
    }

    textureStore(outputTex, out_coord, color / weight_sum);
}
