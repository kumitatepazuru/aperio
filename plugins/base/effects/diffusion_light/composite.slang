import aperio_bindings;

struct CompositeParams {
    float strength;
};

struct InputTextures { Texture2D<float4> tex0; Texture2D<float4> tex1; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<CompositeParams>> res;

// inputs.tex0: このラウンドのボックスぼかし結果(YCbCr+alpha、ストレート、
//              common/box_blur_dir.slang通過済み)
// inputs.tex1: このラウンド開始時点の「現在の画像」を common/expand.slang で
//              同じキャンバスへ再配置したもの(同じくYCbCr+alpha、ストレート)
//
// ぼかし結果の輝度がその画素自身の輝度より明るいときだけ、差分に応じて
// 色差をブレンドしたグロー色を「現在の画像」の上に強さ(strength)を
// アルファとして source-over で重ねる。暗くなる方向には働かない。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let blur_tex = inputs.tex0;
    uint dimsU, dimsV;
    blur_tex.GetDimensions(dimsU, dimsV);
    let dims = int2(dimsU, dimsV);
    let coord = int2(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let blur = blur_tex.Load(int3(coord, 0));
    let src = inputs.tex1.Load(int3(coord, 0));

    let gy = blur.b;
    let premul_y = max(0.0, src.b * src.a);
    let d = gy - premul_y;

    var ga: float = 0.0;
    var mix_cr: float = src.r;
    var mix_cb: float = src.g;
    var mix_y: float = src.b;
    if (gy > 1e-6 && d > 1e-6) {
        ga = clamp(blur.a * res.params[0].strength, 0.0, 1.0);
        let base = gy - d;
        mix_cr = ((src.r * src.a) * base + d * blur.r) / gy;
        mix_cb = ((src.g * src.a) * base + d * blur.g) / gy;
        mix_y = gy;
    }

    // グロー(色=mix、アルファ=ga)を src の上に通常の source-over で合成する。
    let ia = 1.0 - ga;
    let out_a = clamp(ga + src.a * ia, 0.0, 1.0);

    var out_cr: float = src.r;
    var out_cb: float = src.g;
    var out_y: float = src.b;
    if (out_a > 1e-6) {
        let w_glow = ga / out_a;
        let w_src = ia * src.a / out_a;
        out_cr = mix_cr * w_glow + src.r * w_src;
        out_cb = mix_cb * w_glow + src.g * w_src;
        out_y = mix_y * w_glow + src.b * w_src;
    }

    res.outputTex[coord] = float4(out_cr, out_cb, out_y, out_a);
}
