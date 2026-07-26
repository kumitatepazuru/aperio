struct CurveParams {
    base: f32,
    inverse: i32,
    // 変換対象のチャンネル(0=r, 1=g, 2=b, 3=a)。他のチャンネルは素通しする。
    channel: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: CurveParams;

// べき平均カーブの往復変換。ぼかし前に対象量(0~1程度の値)を
// pow(base, r*256)-1 で指数的に持ち上げ、ぼかし後に対数で戻す
// (ちょうど逆関数になっている)。baseが1より大きいほど、ぼかしカーネル内で
// 最も大きい値の寄与が単純平均より強く効くようになる(LogSumExp的な
// softmaxに近い挙動)。発光の拡散速度・ぼかしの光の強さで共通の式
// (どちらもREADMEで実機ヘルパー関数が同一と確認済み)。対象外のチャンネルは
// 素通しする。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let s = textureLoad(tex, coord, 0);
    // 動的な添字での書き込み(s[params.channel] = ...)はHLSLバックエンドへの
    // トランスパイル時に「配列参照をl-valueにできない」というエラーになる
    // (naga/wgpuのDX12経路の既知の制約)ため、one-hotマスクとdot積で
    // 動的な読み書きを避ける。
    // 参照: https://github.com/gfx-rs/wgpu/issues/4460
    let mask = vec4<f32>(
        select(0.0, 1.0, params.channel == 0),
        select(0.0, 1.0, params.channel == 1),
        select(0.0, 1.0, params.channel == 2),
        select(0.0, 1.0, params.channel == 3),
    );
    let r_in = dot(s, mask);
    var r = r_in;
    if (params.inverse == 0) {
        r = pow(params.base, r * 256.0) - 1.0;
    } else {
        r = log(max(r + 1.0, 1e-6)) / (256.0 * log(params.base));
    }
    let out = s + mask * (r - r_in);

    textureStore(outputTex, coord, out);
}
