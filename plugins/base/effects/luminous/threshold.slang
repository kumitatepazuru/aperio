import aperio_color;
import aperio_bindings;

struct ThresholdParams {
    float gain;
    float overflow;
    float threshold;
    // 0: 光色指定モード(cr_dev/cb_dev/luma_colorの定数係数を使う)
    // それ以外: 光色「指定なし」モード(元画素自身の色差を使い、輝度係数=1.0)
    int use_source_chroma;
    float cr_dev;
    float cb_dev;
    float luma_color;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<ThresholdParams>> res;

// 明部抽出(実機 sub_100535a0/sub_100536d0、README手順3)。
//
// 輝度がしきい値を超えた量にgain(強さ100%=1.0)を掛け、100%を超えた分は別枠の
// overflowとして加算する(強さ>100%で、しきい値ぎりぎりの暗めのピクセルまで
// 底上げされて光り出すAviUtl版の挙動を再現)。
//
// 重要: 輝度は**アルファ加重**してからしきい値と比較する(実機の非対称。色差側は
// アルファ加重しない)。aperioはストレートアルファで、text_render.wgslの fs_mask が
// カバレッジ0の画素にもテキスト色をそのまま残すため、これをしないとグリフ矩形の
// 内側全域が輝度1.0として抽出され「白いもや」になる。
//
// 出力は後段のぼかしに乗せる係数付きの抽出量:
//   .r = amount * Cr係数   (色差・後で combined_init が offset を足す)
//   .g = amount * Cb係数
//   .b = amount * 輝度係数 (色指定なら光色のYを既にここで掛ける = 中核(2))
//   .a = amount            (デバッグ用。combined_init は r/g/b しか読まない)
// Cr/Cb/輝度の各係数は光色モードで切り替わる。reconstruct.slang の BT.601 逆変換が
// どちらのモードでも最終色を復元する。
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

    let color = tex.Load(int3(coord, 0));
    let ycc = bt601_encode(color.rgb);
    let y_src = ycc.z;
    let luminance = y_src * color.a;

    var amount: float = 0.0;
    if (luminance > res.params[0].threshold) {
        amount = clamp((luminance - res.params[0].threshold) * res.params[0].gain + res.params[0].overflow, 0.0, 1.0);
    }

    var cr_coeff: float = res.params[0].cr_dev;
    var cb_coeff: float = res.params[0].cb_dev;
    var luma_coeff: float = res.params[0].luma_color;
    if (res.params[0].use_source_chroma != 0) {
        // 指定なし: 元画素自身の色差(アルファ非加重の素の色から算出)、輝度係数=1.0
        cr_coeff = ycc.x;
        cb_coeff = ycc.y;
        luma_coeff = 1.0;
    }

    res.outputTex[coord] = float4(amount * cr_coeff, amount * cb_coeff, amount * luma_coeff, amount);
}
