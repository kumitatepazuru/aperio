import aperio_color;
import aperio_bindings;

struct ExtractParams {
    // T = しきい値(0.0〜2.0に正規化。1.0 = 満輝度)
    float threshold;
    // 1 = 光色「指定なし」(元画素自身の色で光る、ソフトニー付き)
    // 0 = 光色指定(ソースの輝度だけを読み、指定色に掛ける、単純ヒンジ)
    int use_source_color;
    float color_r;
    float color_g;
    float color_b;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<ExtractParams>> res;

// グローの明部抽出(exedit-inspect README `inspect/glow/README.md` §3)。
//
// 「指定なし」はソフトニー(y<=T:0, T<y<2T:2*(y-T), y>=2T:y)で、十分明るい画素では
// 恒等写像に漸近しつつ、元の色相・彩度比を保つ。「指定あり」は単純なヒンジ
// (max(0, y-T))で、しきい値ぶんのオフセットがそのまま残る(README通り、しきい値を
// 上げると発光全体が暗くなる)。
//
// どちらも出力は premultiplied な「発光量」RGB(alphaは以降未使用)。「指定なし」の
// premultiplied 出力は線形性から src.rgb*src.a*(b/y) に一致し、これは
// src.rgb/luma(src.rgb) * b(色相・彩度のみを保った単位色)に等しい形へ整理できる。
float softknee(float y, float t) {
    if (y <= t) {
        return 0.0;
    }
    let y2 = 2.0 * t;
    if (y < y2) {
        return 2.0 * (y - t);
    }
    return y;
}

[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let tex = inputs.tex0;
    uint dimsU, dimsV;
    tex.GetDimensions(dimsU, dimsV);
    let dims = int2(dimsU, dimsV);
    let coord = int2(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let src = tex.Load(int3(coord, 0));
    let luma_straight = bt601_luma(src.rgb);
    let y = luma_straight * src.a;

    var light_rgb: float3 = float3(0.0, 0.0, 0.0);
    if (res.params[0].use_source_color != 0) {
        let amount = softknee(y, res.params[0].threshold);
        if (luma_straight > 1e-6) {
            light_rgb = (src.rgb / luma_straight) * amount;
        }
    } else {
        let amount = max(0.0, y - res.params[0].threshold);
        light_rgb = float3(res.params[0].color_r, res.params[0].color_g, res.params[0].color_b) * amount;
    }

    res.outputTex[coord] = float4(light_rgb, 0.0);
}
