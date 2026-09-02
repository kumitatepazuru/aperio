import aperio_blur;
import aperio_bindings;

struct ColorKeyBorderPass3Params {
    int radius;
    int out_width;
    int out_height;
    // 伸張定数(exedit-inspect color_key README §5「境界補正 ― アルファそのものをぼかす」)。
    // a = 1 - 1/r。v=1(完全不透明)のとき t(v)=1になるコントラスト伸張カーブの係数。
    float a_const;
};

struct InputTextures { Texture2D<float4> tex0; Texture2D<float4> tex1; Texture2D<float4> tex2; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<ColorKeyBorderPass3Params>> res;

// `境界補正 > 0` のパス3(exedit-inspect color_key README §5)。
// inputs.tex0: パス2で垂直方向にボックス平均済みのa0(このパスで水平方向にも平均する)
// inputs.tex1: パス1そのまま(未ぼかし)のa0
// inputs.tex2: 元画像(color + alpha)。rgbはここからそのまま素通しする
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let blurred_tex = inputs.tex0;
    let raw_a0_tex = inputs.tex1;
    let orig_tex = inputs.tex2;
    let coord = int2(global_id.xy);

    if (coord.x >= res.params[0].out_width || coord.y >= res.params[0].out_height) {
        return;
    }

    let radius = res.params[0].radius;
    let sum = plain_box_sum(blurred_tex, coord, int2(1, 0), radius);
    let avg = sum.x / float(2 * radius + 1);

    let a0 = raw_a0_tex.Load(int3(coord, 0)).x;

    // README §5/§6: 「ぼかす対象も掛ける相手も画素のアルファ本体」。マットではなくa0自身を
    // 二重ボックス平均した値とa0の積を取るため、半透明な一様領域でも v = a0²/4096 に潰れうる。
    let r_f = float(radius);
    let bs = border_stretch(avg, a0, r_f, res.params[0].a_const);
    let alpha = a0 * bs.factor;

    let src = orig_tex.Load(int3(coord, 0));
    res.outputTex[coord] = float4(src.rgb, alpha);
}
