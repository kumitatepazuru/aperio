import aperio_color;
import aperio_blur;
import common;
import aperio_bindings;

struct ChromaKeyBorderPass3Params {
    int radius;
    int out_width;
    int out_height;
    // 伸張定数(exedit-inspect chroma_key README §7「パス3-積とコントラスト伸張」)。
    // a = 1 - 1/r。v=1(完全不透明)のとき t(v)=1になるコントラスト伸張カーブの係数。
    float a_const;
    float key_cb;
    float key_cr;
    float key_sat;
    int color_correction;
    int alpha_correction;
};

struct InputTextures { Texture2D<float4> tex0; Texture2D<float4> tex1; Texture2D<float4> tex2; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<ChromaKeyBorderPass3Params>> res;

// `境界補正 > 0` のパス3(exedit-inspect chroma_key README §7)。
// inputs.tex0: パス2で垂直方向にボックス平均済みの map_b/map_c(このパスで水平方向にも
//              平均し、二重ボックス平均された map_a とする)
// inputs.tex1: パス1そのまま(未ぼかし)の map_b/map_c。map_bはそのピクセルの生値との
//              積に、map_cは`色彩補正`時の色差補正係数に使う
// inputs.tex2: 元画像(color + alpha)
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let blurred_tex = inputs.tex0;
    let raw_map_tex = inputs.tex1;
    let orig_tex = inputs.tex2;
    let coord = int2(global_id.xy);

    if (coord.x >= res.params[0].out_width || coord.y >= res.params[0].out_height) {
        return;
    }

    let radius = res.params[0].radius;
    let sum = plain_box_sum(blurred_tex, coord, int2(1, 0), radius);
    let avg_b = sum.x / float(2 * radius + 1);

    let raw_map = raw_map_tex.Load(int3(coord, 0));
    let map_b = raw_map.x;
    let map_c = raw_map.y;

    // README: 「ブレンドではなく積」。二重ボックス平均した値と、このピクセル自身の
    // 生の map_b との積を取ることで、キー側(map_b=0)は平均後もにじまず0のまま保たれる。
    let r_f = float(radius);
    let bs = border_stretch(avg_b, map_b, r_f, res.params[0].a_const);

    let src = orig_tex.Load(int3(coord, 0));
    var alpha: float = src.a * bs.factor;

    let ycc = bt601_encode(src.rgb);
    let cr = ycc.x;
    let cb = ycc.y;
    let y = ycc.z;
    let sat = max(abs(cb), abs(cr));

    var base_term: float = bs.t;
    if (res.params[0].color_correction != 0) {
        base_term = map_c;
    }
    let f = unmix_factor(base_term, sat, res.params[0].key_sat);
    let unmixed = unmix(cb, cr, res.params[0].key_cb, res.params[0].key_cr, f);

    if (res.params[0].color_correction != 0 && res.params[0].alpha_correction != 0 && f > 1e-4 && f < 1.0) {
        alpha = alpha * f;
    }

    let rgb = bt601_decode(unmixed.y, unmixed.x, y);
    res.outputTex[coord] = float4(rgb, alpha);
}
