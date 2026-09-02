import aperio_color;
import aperio_bindings;

struct MediaCompositeModeParams {
    // 0=色情報を上書き, 1=輝度をアルファ値として上書き, 2=輝度をアルファ値として乗算
    int mode;
};

struct InputTextures { Texture2D<float4> tex0; Texture2D<float4> tex1; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<MediaCompositeModeParams>> res;

// exedit-inspect composite_video/composite_image README 共通の3合成モード。
// inputs.tex0=配置済みメディア(対象と同じキャンバスサイズ、矩形外・透明画素はalpha=0)、
// inputs.tex1=対象オブジェクト。matches both effects (`composite_video`, `composite_image`)。
// メディア側alpha=0の画素は「矩形の外」と「メディア自体の透明画素」を区別しないため、
// どちらも対象を透明にクロップする(README「矩形外はalpha=0にクロップされる」の簡略化)。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let media_tex = inputs.tex0;
    let base_tex = inputs.tex1;
    uint dimsU, dimsV;
    base_tex.GetDimensions(dimsU, dimsV);
    let dims = int2(dimsU, dimsV);
    let coord = int2(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let src = media_tex.Load(int3(coord, 0));
    let base = base_tex.Load(int3(coord, 0));

    var out: float4;
    if (src.a <= 0.0) {
        out = float4(0.0, 0.0, 0.0, 0.0);
    } else if (res.params[0].mode == 0) {
        out = float4(src.rgb, base.a);
    } else if (res.params[0].mode == 1) {
        out = float4(base.rgb, clamp(bt601_luma(src.rgb), 0.0, 1.0));
    } else {
        out = float4(base.rgb, clamp(bt601_luma(src.rgb) * base.a, 0.0, 1.0));
    }

    res.outputTex[coord] = out;
}
