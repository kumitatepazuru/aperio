struct JfaStepParams {
    step: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: JfaStepParams;

// ジャンプフラッディング法(JFA)の1ステップ。各ピクセルについて、
// stepだけ離れた8近傍(と自分自身)が持つ「最寄りの透明ピクセル座標」を比較し、
// 最も近いものに更新していく。これをstepを半分にしながら繰り返すことで、
// O(log(画像サイズ))回のパスで各ピクセルの最寄りの透明ピクセルに収束する。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let step = params.step;
    let coord_f = vec2<f32>(coord);

    var best_seed = textureLoad(tex, coord, 0).xy;
    var best_dist = distance(coord_f, best_seed);

    for (var oy = -1; oy <= 1; oy++) {
        for (var ox = -1; ox <= 1; ox++) {
            if (ox == 0 && oy == 0) {
                continue;
            }
            let ncoord = coord + vec2<i32>(ox, oy) * step;
            if (ncoord.x < 0 || ncoord.x >= dims.x || ncoord.y < 0 || ncoord.y >= dims.y) {
                continue;
            }
            let nseed = textureLoad(tex, ncoord, 0).xy;
            let d = distance(coord_f, nseed);
            if (d < best_dist) {
                best_dist = d;
                best_seed = nseed;
            }
        }
    }

    textureStore(outputTex, coord, vec4<f32>(best_seed, 0.0, 0.0));
}
