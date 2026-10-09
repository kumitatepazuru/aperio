import aperio_color;
import aperio_bindings;

struct SharpCompositeParams {
    // strength_ui/100.0(UI 100.0 = ×1.0、README §3)。
    float strength;
};

struct InputTextures { Texture2D<float4> tex0; Texture2D<float4> tex1; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<SharpCompositeParams>> res;

// アンシャープマスクの合成本体(exedit-inspect sharp README §5)。inputs.tex0は
// 元画像(ストレートRGB)、inputs.tex1は4パスのボックスぼかし後(ストレートRGB、
// common/box_blur_dir.slang)。ボックスぼかしはアルファ加重平均でBT.601変換と
// 可換なので、ぼかし自体はRGBのまま行い、YCbCrへはこの合成の直前でのみ変換する
// (実機は先にYCbCrへ変換してからぼかすが結果は同じ)。
//
// p==orig.a*blur.a==1(両方不透明)の高速路は一般式に代数的に一致するため分岐は
// 無い(README §5)。p<1/4096(ほぼ完全透明)のときだけぼかしを捨てて元画像を
// そのまま返す(README §5.3)。アルファは常に元画像のもの。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let orig_tex = inputs.tex0;
    uint dimsU, dimsV;
    orig_tex.GetDimensions(dimsU, dimsV);
    let dims = int2(dimsU, dimsV);
    let coord = int2(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let orig = orig_tex.Load(int3(coord, 0));
    let blur = inputs.tex1.Load(int3(coord, 0));

    let orig_ycc = bt601_encode(orig.rgb);
    let blur_ycc = bt601_encode(blur.rgb);

    let p = orig.a * blur.a;
    var ycc: float3 = orig_ycc;
    if (p >= 1.0 / 4096.0) {
        let diff = (orig_ycc * orig.a - blur_ycc * blur.a) / p;
        ycc = orig_ycc + res.params[0].strength * diff;
    }

    var cr: float = ycc.x;
    var cb: float = ycc.y;
    var y: float = ycc.z;

    // README §6: 負の輝度はクランプせず彩度を落として黒へ着地させる。
    if (y <= -0.25) {
        y = 0.0;
        cb = 0.0;
        cr = 0.0;
    } else if (y < 0.0) {
        let t = (y + 0.25) / 0.25;
        cr *= t;
        cb *= t;
        y = 0.0;
    }

    let rgb = bt601_decode(cr, cb, y);
    res.outputTex[coord] = float4(rgb, orig.a);
}
