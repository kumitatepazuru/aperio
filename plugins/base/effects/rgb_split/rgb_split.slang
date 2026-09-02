import aperio_math;
import aperio_bindings;

struct RgbSplitParams {
    // ずれベクトル(整数px、角度から round() 済み)。pair の一方の
    // チャンネルは +shift、もう一方は -shift の位置からサンプルする。
    int shift_x;
    int shift_y;
    // 0=赤緑, 1=赤青, 2=緑青。
    int pair;
    // 0=A, 1=B(exedit-inspect rgb_split README §7。正規化の順序だけが違う)。
    int variant;
    float strength;
    // 拡張後キャンバス座標 → 元画像座標への平行移動(元画像は拡張前のサイズのまま)。
    int offset_x;
    int offset_y;
    int out_width;
    int out_height;
};

struct InputTextures { Texture2D<float4> tex0; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<RgbSplitParams>> res;

// exedit-inspect rgb_split README: R/G/Bのうち2チャンネルを反対方向へ`ずれ幅`だけ
// シフトしてサンプルし(もう1チャンネルは無変更)、`強さ`で無シフトの元画像と
// アルファ重み付きでクロスフェードする(README §7)。原典はYCbCr往復変換込みだが、
// aperioはRGBネイティブなパイプラインなので行列そのものは不要 ―― チャンネルごとに
// 別位置サンプルし、README通りのアルファ重み付き累積・正規化(A/B系で順序が違う)
// だけを行えば同じ見た目になる。キャンバス外からのサンプルは透明として扱う。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let tex = inputs.tex0;
    uint dimsU, dimsV;
    tex.GetDimensions(dimsU, dimsV);
    let dims = int2(dimsU, dimsV);
    let out_coord = int2(global_id.xy);

    if (out_coord.x >= res.params[0].out_width || out_coord.y >= res.params[0].out_height) {
        return;
    }

    let base = out_coord - int2(res.params[0].offset_x, res.params[0].offset_y);
    let shift = int2(res.params[0].shift_x, res.params[0].shift_y);

    let orig = load_or_zero(tex, base.x, base.y, dims);
    let pos_a = base + shift;
    let pos_b = base - shift;
    let sample_a = load_or_zero(tex, pos_a.x, pos_a.y, dims);
    let sample_b = load_or_zero(tex, pos_b.x, pos_b.y, dims);

    // exedit-inspect rgb_split README §7: 3ブロック(+shift, -shift, 元画像)を
    // アルファ重み付きで足し込んでから正規化する。動く2チャンネルは
    // shift位置のサンプルとstrengthの重みで、静止チャンネルは元画像の値
    // (strength/(1-strength)どちらの寄与も同じ値になる)を使う。
    let t = clamp(res.params[0].strength, 0.0, 1.0);
    let w_a = sample_a.a * t;
    let w_b = sample_b.a * t;
    let w_static = orig.a * t;
    let w_orig = orig.a * (1.0 - t);

    var accum: float3;
    if (res.params[0].pair == 0) {
        // 赤緑: R=+shift, G=-shift, B=静止
        accum = float3(
            sample_a.r * w_a + orig.r * w_orig,
            sample_b.g * w_b + orig.g * w_orig,
            orig.b * (w_static + w_orig),
        );
    } else if (res.params[0].pair == 1) {
        // 赤青: R=+shift, B=-shift, G=静止
        accum = float3(
            sample_a.r * w_a + orig.r * w_orig,
            orig.g * (w_static + w_orig),
            sample_b.b * w_b + orig.b * w_orig,
        );
    } else {
        // 緑青: G=+shift, B=-shift, R=静止
        accum = float3(
            orig.r * (w_static + w_orig),
            sample_a.g * w_a + orig.g * w_orig,
            sample_b.b * w_b + orig.b * w_orig,
        );
    }

    let a_sum = w_a + w_b + w_static + 3.0 * w_orig;

    var out_rgb = accum;
    var out_a: float;
    if (res.params[0].variant == 0) {
        // A系: 被覆率が最大(=3)未満のときだけ割り戻す。
        if (a_sum > 0.0 && a_sum < 3.0) {
            out_rgb = accum / a_sum;
        }
        out_a = a_sum / 3.0;
    } else {
        // B系: 先にアルファを1/3にしてから割り戻す(縁の色がA系より最大3倍濃くなる)。
        let e = a_sum / 3.0;
        if (e > 0.0) {
            out_rgb = accum / e;
        }
        out_a = e;
    }

    res.outputTex[out_coord] = float4(clamp(out_rgb, float3(0.0, 0.0, 0.0), float3(1.0, 1.0, 1.0)), clamp(out_a, 0.0, 1.0));
}
