import aperio_math;
import aperio_bindings;

struct CombinedInitParams {
    float base;
    float offset;
    // 0: 拡散速度0(輝度は線形のまま)。それ以外: 拡散速度>0(輝度に指数カーブを掛ける)
    int apply_curve;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<CombinedInitParams>> res;

// threshold.slang が作った抽出量(r=amount*Cr係数, g=amount*Cb係数, b=amount*輝度係数)
// を、6パスぼかしに乗せられる形へ整える初期化シェーダー。輝度・色差を1本の
// チェーンにまとめて回すための共通の入口(拡散速度0/>0の両方が通る)。
//
//   .r = in.r + offset   (色差。box_blur_dir.slangが負値をmax(0,x)で潰すため十分大きい
//   .g = in.g + offset    offsetを足しておき、蓄積時に差し引く。輝度は非負なので不要)
//   .b = apply_curve ? pow(base, in.b*256)-1 : in.b
//        (拡散速度>0のときだけ指数カーブ。in.b は既に amount*輝度係数 なので
//         光色のYを先に掛けた値がそのまま指数の肩に乗る = 中核(2))
//   .a = 1.0 (box_blur_dir.slangのプリマルチプライドをno-opにして、r/g/bをそれぞれ
//        独立な単純重み平均としてぼかすためのダミー値)
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
    let cr = src.r + res.params[0].offset;
    let cb = src.g + res.params[0].offset;
    var luma: float = src.b;
    if (res.params[0].apply_curve != 0) {
        luma = curve_forward(res.params[0].base, src.b);
    }

    res.outputTex[coord] = float4(cr, cb, luma, 1.0);
}
