import aperio_bindings;

struct EncodePatternParams {
    // 濃さ/強さに相当する係数(0..1)。`シャドー`は濃さトラックバー由来、
    // `縁取り`にはこれに相当するパラメータが無いため常に1.0を渡す。
    float density;
};

struct InputTextures { Texture2D<float4> tex0; Texture2D<float4> tex1; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<EncodePatternParams>> res;

// パターン画像版の共通エンコード。inputs.tex0は被覆率マップ(.aだけ使う、
// `シャドー`は4パスのボックス平均、`縁取り`は2パスの「割らないボックス和+gain」)、
// inputs.tex1はタイル済みパターン画像(common/tile.slangの出力、ストレートRGBA)。
// 色パラメータは完全に無視し、パターンの色をそのまま使う。アルファ同士が
// 掛かるだけなので、density とパターンの透明度は交換可能。
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
    let pattern = inputs.tex1.Load(int3(coord, 0));
    let alpha = clamp(avg * pattern.a * res.params[0].density, 0.0, 1.0);

    res.outputTex[coord] = float4(pattern.rgb, alpha);
}
