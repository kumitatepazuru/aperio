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

    // 垂直オフセット: 出力は入力より上下それぞれ radius 分高い
    let in_coord = out_coord - vec2<i32>(0, radius);

    if (radius <= 0) {
        // 水平パスからのプリマルチプライドを un-premultiply して出力
        let premul = textureLoad(tex, in_coord, 0);
        var out_color: vec4<f32>;
        if (premul.a > 1e-6) {
            out_color = vec4(premul.rgb / premul.a, premul.a);
        } else {
            out_color = vec4(0.0);
        }
        textureStore(outputTex, out_coord, out_color);
        return;
    }

    // 入力は水平パスからのプリマルチプライドアルファ
    // そのまま蓄積してから最後に un-premultiply する
    let sigma2 = 2.0 * pow(f32(radius) / 3.0, 2.0);

    var color = vec4<f32>(0.0);
    var weight_sum = 0.0;

    for (var dy = -radius; dy <= radius; dy++) {
        let weight = exp(-f32(dy * dy) / sigma2);
        weight_sum += weight;
        let sample_coord = in_coord + vec2<i32>(0, dy);
        if (sample_coord.x >= 0 && sample_coord.x < in_dims.x &&
            sample_coord.y >= 0 && sample_coord.y < in_dims.y) {
            // 入力はすでにプリマルチプライド済みなのでそのまま加算
            color += textureLoad(tex, sample_coord, 0) * weight;
        }
        // 境界外は vec4(0) 扱い、ただし weight_sum には加算 → エッジが透明にフェード
    }

    let premul = color / weight_sum;
    var out_color: vec4<f32>;
    if (premul.a > 1e-6) {
        out_color = vec4(premul.rgb / premul.a, premul.a);
    } else {
        out_color = vec4(0.0);
    }

    textureStore(outputTex, out_coord, out_color);
}
