import aperio_blur;
import aperio_bindings;

struct BoxBlurAlphaMergeParams {
    int radius;
    int out_width;
    int out_height;
    int offset;
    int border_mode;
    int divisor_mode;
};

struct InputTextures { Texture2D<float4> tex0; Texture2D<float4> tex1; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<BoxBlurAlphaMergeParams>> res;

// common/lib/aperio-blur.slang の box_sum (common/box_blur_dir.slang と共用) の水平パスと
// alpha_merge.slang の非線形しきい値合成を1シェーダーに統合したもの(境界ぼかしの
// 透明度境界モード専用)。alpha_merge.slang が読むのはぼかし後の.aだけなので、
// box_sumが返すsum_rgbはここでは使わない。
//
// inputs.tex0: 垂直パス済みのアルファ(box_blur_dir通過後。ry<=0なら元画像そのもの)
// inputs.tex1: 元画像(色 + 元のアルファA)
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let tex = inputs.tex0;
    let orig_tex = inputs.tex1;
    let out_coord = int2(global_id.xy);
    let radius = res.params[0].radius;

    if (out_coord.x >= res.params[0].out_width || out_coord.y >= res.params[0].out_height) {
        return;
    }

    let in_coord = out_coord - int2(res.params[0].offset, 0);

    let r = box_sum(tex, in_coord, int2(1, 0), radius, res.params[0].border_mode);

    let divisor = res.params[0].divisor_mode != 0 ? float(max(r.valid_count, 1)) : float(2 * radius + 1);
    let b = r.sum_weight / divisor;

    // README 4: 単純なボックスぼかし結果をそのまま使うのではなく、非線形なしきい値
    // カーブで再合成する(実機の12bit整数演算 t=((A*B)>>11)-4096;
    // alpha'=(A*t)>>12 if t>0 else 0 を [0,1] 正規化した連続式)。
    let orig = orig_tex.Load(int3(out_coord, 0));
    let a = orig.a;
    let t = 2.0 * a * b - 1.0;
    let alpha = max(a * t, 0.0);

    res.outputTex[out_coord] = float4(orig.rgb, alpha);
}
