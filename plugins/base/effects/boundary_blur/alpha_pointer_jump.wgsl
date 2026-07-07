@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

// ポインタジャンプ(倍々法)。各ピクセルのポインタを「ポインタ先のポインタ」に
// 置き換えることを繰り返し、log2(パス長)回程度で島の中心点(極大点)まで一気に収束させる。
// 自身が持つ距離D(z成分)はポインタを辿っても変化しないのでそのまま引き継ぐ。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let self_data = textureLoad(tex, coord, 0);
    let next_coord = clamp(
        vec2<i32>(i32(round(self_data.x)), i32(round(self_data.y))),
        vec2<i32>(0, 0),
        dims - vec2<i32>(1, 1)
    );
    let jumped = textureLoad(tex, next_coord, 0);

    textureStore(outputTex, coord, vec4<f32>(jumped.x, jumped.y, self_data.z, 0.0));
}
