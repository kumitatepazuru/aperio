import aperio_bindings;

struct ShadowApplyParams {
    // 0 = 逆光オフ(素のアルファ平均、+4096バイアス相当)
    // 1 = 逆光オン(反転アルファ平均、バイアス無し)
    int backlight;
    // 陰影係数(強さ・比率から算出した0..1の割合)
    float b;
    float color_r;
    float color_g;
    float color_b;
};

struct InputTextures { Texture2D<float4> tex0; Texture2D<float4> tex1; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<ShadowApplyParams>> res;

// 陰影パス本体(exedit-inspect light README §3)。inputs.tex0は元オブジェクト
// (base rgb+alpha, straight)、inputs.tex1はボックス平均済みバッファ(.aだけ使う。
// 逆光オフなら生アルファの平均、オンならinvert_alpha.slang経由の反転アルファの平均)。
//
// 実機はY/Cb/Crの3チャンネルへ加算するが、BT.601変換は線形写像(オフセット無し)
// なので decode(Y+dY, Cb+dCb, Cr+dCr) = decode(Y,Cb,Cr) + light_rgb*m と等価になる
// (light_rgb = 光色そのもの)。したがってYCbCr変換を経由せず、光色RGBを直接
// baseへ加算するだけでよい(glint.slangが同種の理由でRGB空間のまま計算しているのと
// 同じ理屈)。アルファには一切触れない。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let base_tex = inputs.tex0;
    uint dimsU, dimsV;
    base_tex.GetDimensions(dimsU, dimsV);
    let dims = int2(dimsU, dimsV);
    let coord = int2(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let base = base_tex.Load(int3(coord, 0));
    let avg = inputs.tex1.Load(int3(coord, 0)).a;

    var m: float;
    if (res.params[0].backlight != 0) {
        m = avg * res.params[0].b;
    } else {
        let bs = res.params[0].b * 2.0 / 11.0;
        m = (avg + 1.0) * bs;
    }
    m = clamp(m, 0.0, 1.0);

    let light_color = float3(res.params[0].color_r, res.params[0].color_g, res.params[0].color_b);
    res.outputTex[coord] = float4(base.rgb + light_color * m, base.a);
}
