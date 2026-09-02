import aperio_blur;
import aperio_color;
import common;
import aperio_bindings;

struct BorderPass3Params {
    int radius;
    int out_width;
    int out_height;
    float a_const;
    float key_sat;
    float key_y;
    float after_cr;
    float after_cb;
    float after_y;
};

struct InputTextures { Texture2D<float4> tex0; Texture2D<float4> tex1; Texture2D<float4> tex2; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<BorderPass3Params>> res;

// `境界補正 > 0` のパス3。
// inputs.tex0: パス2で垂直方向に平均済みの距離d(このパスで水平方向にも平均する)
// inputs.tex1: パス1そのまま(未ぼかし)の距離d
// inputs.tex2: 元画像(color + alpha)
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let blurred_tex = inputs.tex0;
    let raw_tex = inputs.tex1;
    let orig_tex = inputs.tex2;
    let coord = int2(global_id.xy);

    if (coord.x >= res.params[0].out_width || coord.y >= res.params[0].out_height) {
        return;
    }

    let radius = res.params[0].radius;
    let sum = plain_box_sum(blurred_tex, coord, int2(1, 0), radius);
    let avg = sum.x / float(2 * radius + 1);

    let raw_d = raw_tex.Load(int3(coord, 0)).x;
    let r_f = float(radius);
    let bs = border_stretch(avg, raw_d, r_f, res.params[0].a_const);

    let src = orig_tex.Load(int3(coord, 0));
    // exedit-inspect convert_gamut README §7: アルファには書き戻さず、伸張済みの t に
    // 画素アルファを掛けたものをそのまま混合率として使う(chroma_keyはこれをalphaに書く)。
    let d_final = clamp(src.a * bs.factor, 0.0, 1.0);

    let ycc = bt601_encode(src.rgb);
    let out_ycc = gamut_apply(
        ycc.x, ycc.y, ycc.z,
        res.params[0].key_sat, res.params[0].key_y,
        res.params[0].after_cr, res.params[0].after_cb, res.params[0].after_y,
        d_final
    );
    let rgb = bt601_decode(out_ycc.x, out_ycc.y, out_ycc.z);

    res.outputTex[coord] = float4(rgb, src.a);
}
