struct SeparableBlurParams {
    radius: i32,
    new_width: i32,
    new_height: i32,
    light_intensity: i32,
    fixed_size: i32,
    // exp(-dx^2 / sigma2) の分母。sigma2 = 2 * sigma^2 (sigmaは通常のガウス標準偏差)。
    // radiusから逆算せず呼び出し側で明示的に渡すことで、
    // 「ぼかし半径(見た目のradius)」と「ガウスの広がり(sigma)」を独立に制御できる。
    sigma2: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;
@group(0) @binding(2) var linear_sampler: sampler;

@group(1) @binding(0) var<storage, read> params_array: SeparableBlurParams;

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

    // サイズ固定時はオフセットなし、通常は出力が左右 radius 分広い
    let x_offset = select(radius, 0, params_array.fixed_size != 0);
    let in_coord = out_coord - vec2<i32>(x_offset, 0);

    // output = (GaussianBlur_sigma(orig^n))^(1/n), n = 1 + 0.103 * 光の強さ ^ 1.2
    // ここでは orig^n を計算してからぼかしを畳み込む (べき乗を戻すのは垂直パスの最後)
    let n = 1.0 + 0.103 * pow(f32(params_array.light_intensity), 1.2);

    let sigma2 = params_array.sigma2;

    var color = vec4<f32>(0.0);
    var weight_sum = 0.0;

    for (var dx = -radius; dx <= radius; dx++) {
        let weight = exp(-f32(dx * dx) / sigma2);
        weight_sum += weight;
        let raw_coord = in_coord + vec2<i32>(dx, 0);
        if (params_array.fixed_size != 0) {
            // サイズ固定時はエッジピクセルをクランプして透明化を防ぐ
            let sc = clamp(raw_coord, vec2<i32>(0, 0), in_dims - vec2<i32>(1, 1));
            let s = textureLoad(tex, sc, 0);
            let powered = pow(max(s.rgb, vec3<f32>(0.0)), vec3<f32>(n));
            color += vec4(powered * s.a, s.a) * weight;
        } else if (raw_coord.x >= 0 && raw_coord.x < in_dims.x &&
                   raw_coord.y >= 0 && raw_coord.y < in_dims.y) {
            let s = textureLoad(tex, raw_coord, 0);
            let powered = pow(max(s.rgb, vec3<f32>(0.0)), vec3<f32>(n));
            color += vec4(powered * s.a, s.a) * weight;
        }
        // 境界外(サイズ非固定時)は vec4(0) 扱い → エッジが透明にフェード
    }

    // プリマルチプライドアルファのまま出力 (垂直パスへの中間テクスチャ)
    textureStore(outputTex, out_coord, color / weight_sum);
}
