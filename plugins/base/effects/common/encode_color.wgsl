enable wgpu_binding_array;

struct EncodeColorParams {
    // 濃さ/強さに相当する係数(0..1)。`シャドー`は濃さトラックバー由来、
    // `縁取り`と`エッジ抽出`にはこれに相当するパラメータが無いため常に1.0を渡す
    // (`エッジ抽出`の`強さ`はマップ側で既に掛かっている)。
    density: f32,
    color_r: f32,
    color_g: f32,
    color_b: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: EncodeColorParams;

// 色版の共通エンコード。inputTex[0]は被覆率マップ(.aだけ使う、[0,1]に正規化済み
// —— `シャドー`(exedit-inspect shadow README §5)は4パスのボックス平均済み
// アルファ、`縁取り`(exedit-inspect border README §5)は2パスの「割らない
// ボックス和+gain」済みの被覆マップ、`エッジ抽出`(exedit-inspect edge_extraction
// README §5.3)はPrewittの勾配強度×中央画素のアルファ)。指定色はそのままY/Cb/Crへ入り
// (アンプリマルチプライ無し)、被覆率×densityだけをアルファに載せる。
// 3つとも光色の暗さが効果の強さに影響しないのはこの形のため(common/rgb_ycbcr.md §4)。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let mask_tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(mask_tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let avg = textureLoad(mask_tex, coord, 0).a;
    let alpha = clamp(avg * params.density, 0.0, 1.0);

    textureStore(outputTex, coord, vec4<f32>(params.color_r, params.color_g, params.color_b, alpha));
}
