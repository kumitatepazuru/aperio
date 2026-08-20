enable wgpu_binding_array;

struct EncodePatternParams {
    // 濃さ/強さに相当する係数(0..1)。`シャドー`は濃さトラックバー由来、
    // `縁取り`にはこれに相当するパラメータが無いため常に1.0を渡す。
    density: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: EncodePatternParams;

// パターン画像版の共通エンコード。inputTex[0]は被覆率マップ(.aだけ使う、
// `シャドー`は4パスのボックス平均、`縁取り`は2パスの「割らないボックス和+gain」)、
// inputTex[1]はタイル済みパターン画像(common/tile.wgslの出力、ストレートRGBA)。
// 色パラメータは完全に無視し、パターンの色をそのまま使う。アルファ同士が
// 掛かるだけなので、density とパターンの透明度は交換可能。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let mask_tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(mask_tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let avg = textureLoad(mask_tex, coord, 0).a;
    let pattern = textureLoad(inputTex[1], coord, 0);
    let alpha = clamp(avg * pattern.a * params.density, 0.0, 1.0);

    textureStore(outputTex, coord, vec4<f32>(pattern.rgb, alpha));
}
