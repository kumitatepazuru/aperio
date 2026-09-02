import aperio_bindings;

struct LightCompositeParams {
    // 後光係数(強さ・比率から算出した0..1の割合)
    float a;
    // 光色をY=1.0で復元したRGB(CPU側で事前計算。ガモット外に出ることがあるので
    // クランプしない)
    float halo_r;
    float halo_g;
    float halo_b;
    // 光色のBT.601輝度(0..1)
    float light_y;
};

struct InputTextures { Texture2D<float4> tex0; Texture2D<float4> tex1; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<LightCompositeParams>> res;

// 後光パスのエンコードと最終合成を1パスに融合したもの(exedit-inspect light
// README §4・§5)。inputs.tex0は拡張済み・陰影適用後オブジェクト(base、src/前面)、
// inputs.tex1は拡張キャンバス上の2Dボックス平均アルファ(.aだけ使う)。
//
// 後光は「輝度をアルファへ移すアンプリマルチプライ」形式でエンコードする
// (README §4: Y=4096固定、Cb/Cr=光色、a=光色Y*scale/4096)。ここではYCbCrを
// 経由せず、光色をY=1.0で復元した固定RGB(halo_r/g/b)にscale*light_yをアルファ
// として載せるだけの等価な形にしてある。
//
// 合成はglint.wglのmode==1(後方に合成/通常合成)と同じ式: base(src/前面)を
// halo(dst/背面)の上に source-over で重ねる。halo_rgbはガモット外に出ることが
// あるが、プリマルチプライドで合成して割り戻すだけなのでクランプしない
// (RGBへのクリップは実機と同じく表示側に任せる)。
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
    let avg2d_a = inputs.tex1.Load(int3(coord, 0)).a;

    let scale = min(1.0, avg2d_a * res.params[0].a);
    let halo_alpha = res.params[0].light_y * scale;
    let halo_rgb = float3(res.params[0].halo_r, res.params[0].halo_g, res.params[0].halo_b);

    let out_alpha = base.a + halo_alpha * (1.0 - base.a);
    let premul = base.rgb * base.a + halo_rgb * halo_alpha * (1.0 - base.a);

    var out_rgb: float3 = float3(0.0, 0.0, 0.0);
    if (out_alpha > 1e-6) {
        out_rgb = premul / out_alpha;
    }

    res.outputTex[coord] = float4(out_rgb, clamp(out_alpha, 0.0, 1.0));
}
