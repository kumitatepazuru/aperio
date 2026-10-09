import aperio_color;
import aperio_bindings;

struct CompositeParams {
    // 0以外: `光成分のみ`。最終合成を丸ごとスキップし、蓄積バッファを不透明
    // キャンバスとしてそのまま出力する(exedit-inspect glow README §8)。
    int light_only;
};

struct InputTextures { Texture2D<float4> tex0; Texture2D<float4> tex1; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<CompositeParams>> res;

// inputs.tex0: 元画像を common/expand.slang で配置したもの(straight rgba)
// inputs.tex1: 最終蓄積(.rgb=発光量。premultiplied相当、輝度がそのままアルファに
//              なる。.aは仕上げ`ぼかし`パスで潰れているため使わず、常にここで
//              bt601_lumaから再計算する)
//
// README §8 の最終合成(オブジェクト版)は「元画像が透明/不透明/半透明」で3分岐
// するが、na=min(g_luma+base.a, 1.0) を軸にした1本の式へ単純化できる:
// base.a=0 では na=min(g_luma,1)、premul/na が (a)の「輝度をアルファへ移す
// アンプリマルチプライ」と一致し、base.a=1 では na=1 となって
// premul/na = base.rgb + accum.rgb と (b)の単純加算に一致する(境界で連続)。
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

    let accum = inputs.tex1.Load(int3(coord, 0)).rgb;
    let g_luma = bt601_luma(accum);

    if (res.params[0].light_only != 0) {
        res.outputTex[coord] = float4(accum, 1.0);
        return;
    }

    let base = base_tex.Load(int3(coord, 0));

    let na = clamp(g_luma + base.a, 0.0, 1.0);
    let premul = base.rgb * base.a + accum;

    var out_rgb: float3 = float3(0.0, 0.0, 0.0);
    if (na > 1e-6) {
        out_rgb = premul / na;
    }

    res.outputTex[coord] = float4(out_rgb, na);
}
