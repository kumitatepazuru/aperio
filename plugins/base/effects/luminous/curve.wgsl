struct CurveParams {
    base: f32,
    inverse: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;
@group(0) @binding(2) var linear_sampler: sampler;

@group(1) @binding(0) var<storage, read> params: CurveParams;

// 拡散速度: ぼかし前に抽出量(alphaに詰めた0~1程度の値)を
// pow(base, r*256)-1 で指数的に持ち上げ、ぼかし後に対数で戻す
// (ちょうど逆関数になっている)。baseが1より大きいほど、ぼかしカーネル内で
// 最も明るい発光源の寄与が単純平均より強く効くようになる(LogSumExp的な
// softmaxに近い挙動)。rgb(光色)は素通しする。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let s = textureLoad(tex, coord, 0);
    var r = s.a;
    if (params.inverse == 0) {
        r = pow(params.base, r * 256.0) - 1.0;
    } else {
        r = log(max(r + 1.0, 1e-6)) / (256.0 * log(params.base));
    }

    textureStore(outputTex, coord, vec4<f32>(s.rgb, r));
}
