#define_import_path aperio::chroma_key

const TWO_PI: f32 = 6.28318530717958647693;
// f(アンミックス係数)・sat比較のゼロ割ガード。exedit-inspect chroma_key README §6の
// 「f>1のゲートがゼロ除算を防ぐ」に相当する連続版のイプシロン。
const EPS: f32 = 1e-4;

struct KeyMetrics {
    sat: f32,
    // README の hue_excess を 4096=1.0 として正規化したもの(色相超過分のみ、彩度の
    // 重みを足す前)。f(アンミックス係数)の計算にも使う。
    hue_excess: f32,
    // README の d を 4096=1.0 として正規化したもの。
    d_norm: f32,
};

// 色差(cb, cr)とキー色から、色相・彩度の距離指標を求める
// (exedit-inspect chroma_key README §4)。cb/cr は aperio::color::bt601_encode の
// 戻り値で [-0.5, 0.5] に正規化されている前提(README の ±2048 に相当)。
fn key_metrics(cb: f32, cr: f32, key_cb: f32, key_cr: f32, key_sat: f32, hue_range_turns: f32, sat_range: f32) -> KeyMetrics {
    let sat = max(abs(cb), abs(cr));

    let hue = atan2(cr, cb);
    let key_hue = atan2(key_cr, key_cb);
    var diff = hue - key_hue;
    diff = diff - TWO_PI * round(diff / TWO_PI);
    let dh_turns = abs(diff) / TWO_PI;

    let hue_excess = 16.0 * max(0.0, dh_turns - hue_range_turns);
    let sat_excess = max(0.0, abs(sat - key_sat) - sat_range);
    let d_norm = hue_excess + 8.0 * sat_excess;

    return KeyMetrics(sat, hue_excess, d_norm);
}

// スピルのアンミックス係数(exedit-inspect chroma_key README §6)。
// f_norm = f/4096 で、前景の被覆率cの見積もりにあたる。
fn unmix_factor(hue_excess: f32, sat: f32, key_sat: f32) -> f32 {
    let sat_ratio = (key_sat - sat) / max(key_sat, EPS);
    return max(hue_excess, sat_ratio);
}

// 色差をキー色から遠ざける(README §6の恒等式 fg = key + (px-key)/c)。
// f が [EPS, 1.0) の範囲でなければ何もせず cb/cr をそのまま返す
// (f>=1.0: 被覆率ほぼ100% = 元々キー色でない、f<=EPS: ゼロ割回避)。
fn unmix(cb: f32, cr: f32, key_cb: f32, key_cr: f32, f: f32) -> vec2<f32> {
    if (f > EPS && f < 1.0) {
        let cb2 = clamp(key_cb + (cb - key_cb) / f, -0.5, 0.5);
        let cr2 = clamp(key_cr + (cr - key_cr) / f, -0.5, 0.5);
        return vec2<f32>(cb2, cr2);
    }
    return vec2<f32>(cb, cr);
}
