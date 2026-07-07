@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

// 島の中心を推定するジャンプフラッディング法(JFA)の種を仕込むパス。
// 透明ピクセル自身を「種」とし、自身の座標を記録する。それ以外は遠方センチネル値にしておく。
const SENTINEL: f32 = 1e6;

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let a = textureLoad(tex, coord, 0).a;

    var seed: vec2<f32>;
    if (a < 0.01) {
        seed = vec2<f32>(coord);
    } else {
        seed = vec2<f32>(SENTINEL, SENTINEL);
    }

    textureStore(outputTex, coord, vec4<f32>(seed, 0.0, 0.0));
}
