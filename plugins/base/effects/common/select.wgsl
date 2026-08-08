enable wgpu_binding_array;

struct SelectParams {
    index: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: SelectParams;

// inputTex[params.index] をそのままoutputTexへコピーするだけの素通しシェーダー。
// パイプラインのstateは1ステップごとに1枚に潰れるため、複数バッファを跨いで
// 保持したい場合(前段の結果を次段が読みつつ、同じ入力を後続の呼び出しでも
// 使い回したい場合)にparallelブランチの中でこれを使って明示的に複製・選別する。
// glow/luminous共通。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[params.index];
    let dims = vec2<i32>(textureDimensions(tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    textureStore(outputTex, coord, textureLoad(tex, coord, 0));
}
