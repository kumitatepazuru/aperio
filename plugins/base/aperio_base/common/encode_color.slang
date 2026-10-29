import aperio_bindings;

struct EncodeColorParams {
    // 濃さ/強さに相当する係数(0..1)。`シャドー`は濃さトラックバー由来、
    // `縁取り`と`エッジ抽出`にはこれに相当するパラメータが無いため常に1.0を渡す
    // (`エッジ抽出`の`強さ`はマップ側で既に掛かっている)。
    float density;
    float color_r;
    float color_g;
    float color_b;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<EncodeColorParams>> res;

// 色版の共通エンコード。inputs.tex0は被覆率マップ(.aだけ使う、[0,1]に正規化済み
// —— `シャドー`(exedit-inspect shadow README §5)は4パスのボックス平均済み
// アルファ、`縁取り`(exedit-inspect border README §5)は2パスの「割らない
// ボックス和+gain」済みの被覆マップ、`エッジ抽出`(exedit-inspect edge_extraction
// README §5.3)はPrewittの勾配強度×中央画素のアルファ)。指定色はそのままY/Cb/Crへ入り
// (アンプリマルチプライ無し)、被覆率×densityだけをアルファに載せる。
// 3つとも光色の暗さが効果の強さに影響しないのはこの形のため(common/rgb_ycbcr.md §4)。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let mask_tex = inputs.tex0;
    uint dimsU, dimsV;
    mask_tex.GetDimensions(dimsU, dimsV);
    let dims = int2(dimsU, dimsV);
    let coord = int2(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let avg = mask_tex.Load(int3(coord, 0)).a;
    let alpha = clamp(avg * res.params[0].density, 0.0, 1.0);

    res.outputTex[coord] = float4(res.params[0].color_r, res.params[0].color_g, res.params[0].color_b, alpha);
}
