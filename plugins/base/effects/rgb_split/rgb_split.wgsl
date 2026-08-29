enable wgpu_binding_array;

#import aperio::math::{load_or_zero}

struct RgbSplitParams {
    // ずれベクトル(整数px、角度から round() 済み)。pair の一方の
    // チャンネルは +shift、もう一方は -shift の位置からサンプルする。
    shift_x: i32,
    shift_y: i32,
    // 0=赤緑, 1=赤青, 2=緑青。
    pair: i32,
    // 0=A, 1=B(exedit-inspect rgb_split README §7。正規化の順序だけが違う)。
    variant: i32,
    strength: f32,
    // 拡張後キャンバス座標 → 元画像座標への平行移動(元画像は拡張前のサイズのまま)。
    offset_x: i32,
    offset_y: i32,
    out_width: i32,
    out_height: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: RgbSplitParams;

// exedit-inspect rgb_split README: R/G/Bのうち2チャンネルを反対方向へ`ずれ幅`だけ
// シフトしてサンプルし(もう1チャンネルは無変更)、`強さ`で無シフトの元画像と
// アルファ重み付きでクロスフェードする(README §7)。原典はYCbCr往復変換込みだが、
// aperioはRGBネイティブなパイプラインなので行列そのものは不要 ―― チャンネルごとに
// 別位置サンプルし、README通りのアルファ重み付き累積・正規化(A/B系で順序が違う)
// だけを行えば同じ見た目になる。キャンバス外からのサンプルは透明として扱う。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let out_coord = vec2<i32>(global_id.xy);

    if (out_coord.x >= params.out_width || out_coord.y >= params.out_height) {
        return;
    }

    let base = out_coord - vec2<i32>(params.offset_x, params.offset_y);
    let shift = vec2<i32>(params.shift_x, params.shift_y);

    let orig = load_or_zero(tex, base.x, base.y, dims);
    let pos_a = base + shift;
    let pos_b = base - shift;
    let sample_a = load_or_zero(tex, pos_a.x, pos_a.y, dims);
    let sample_b = load_or_zero(tex, pos_b.x, pos_b.y, dims);

    // exedit-inspect rgb_split README §7: 3ブロック(+shift, -shift, 元画像)を
    // アルファ重み付きで足し込んでから正規化する。動く2チャンネルは
    // shift位置のサンプルとstrengthの重みで、静止チャンネルは元画像の値
    // (strength/(1-strength)どちらの寄与も同じ値になる)を使う。
    let t = clamp(params.strength, 0.0, 1.0);
    let w_a = sample_a.a * t;
    let w_b = sample_b.a * t;
    let w_static = orig.a * t;
    let w_orig = orig.a * (1.0 - t);

    var accum: vec3<f32>;
    if (params.pair == 0) {
        // 赤緑: R=+shift, G=-shift, B=静止
        accum = vec3<f32>(
            sample_a.r * w_a + orig.r * w_orig,
            sample_b.g * w_b + orig.g * w_orig,
            orig.b * (w_static + w_orig),
        );
    } else if (params.pair == 1) {
        // 赤青: R=+shift, B=-shift, G=静止
        accum = vec3<f32>(
            sample_a.r * w_a + orig.r * w_orig,
            orig.g * (w_static + w_orig),
            sample_b.b * w_b + orig.b * w_orig,
        );
    } else {
        // 緑青: G=+shift, B=-shift, R=静止
        accum = vec3<f32>(
            orig.r * (w_static + w_orig),
            sample_a.g * w_a + orig.g * w_orig,
            sample_b.b * w_b + orig.b * w_orig,
        );
    }

    let a_sum = w_a + w_b + w_static + 3.0 * w_orig;

    var out_rgb = accum;
    var out_a: f32;
    if (params.variant == 0) {
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

    textureStore(outputTex, out_coord, vec4<f32>(clamp(out_rgb, vec3<f32>(0.0), vec3<f32>(1.0)), clamp(out_a, 0.0, 1.0)));
}
