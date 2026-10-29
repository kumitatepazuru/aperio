import aperio_bindings;

struct InputTextures { Texture2D<float4> tex0; Texture2D<float4> tex1; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResourcesNoParams> res;

// 2枚のストレートアルファ画像を標準的なプリマルチプライドsource-overで合成する
// 汎用ステップ。inputs.tex0が前面(src)、inputs.tex1が背面(dst)。
// `シャドー`(exedit-inspect shadow README §3。前面=キャンバスへ配置し直した
// オブジェクト、背面=影レイヤー)と`縁取り`(exedit-inspect border README §5。
// 前面=キャンバス中央に配置し直したオブジェクト、背面=縁レイヤー)が共通で使う。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let front_tex = inputs.tex0;
    uint dimsU, dimsV;
    front_tex.GetDimensions(dimsU, dimsV);
    let dims = int2(dimsU, dimsV);
    let coord = int2(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let front = front_tex.Load(int3(coord, 0));
    let back = inputs.tex1.Load(int3(coord, 0));

    let out_alpha = front.a + back.a * (1.0 - front.a);
    let premul = front.rgb * front.a + back.rgb * back.a * (1.0 - front.a);

    var out_rgb: float3 = float3(0.0, 0.0, 0.0);
    if (out_alpha > 1e-6) {
        out_rgb = premul / out_alpha;
    }

    res.outputTex[coord] = float4(out_rgb, clamp(out_alpha, 0.0, 1.0));
}
