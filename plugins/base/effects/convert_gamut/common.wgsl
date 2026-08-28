#define_import_path aperio::convert_gamut

const TWO_PI: f32 = 6.28318530717958647693;

// exedit-inspect convert_gamut README §1/§5: 判定部はクロマキーと同じ距離式
// (d = 色相超過 + 8*彩度超過)を使う。aperio::chroma_key::key_metrics と同じ
// 重み付き和で、hue_range_turns は「1周を1.0とした色相範囲」(= raw/512)、
// sat_range は key_sat に対する彩度範囲(= raw/256 * key_sat)。
// 戻り値は[0,1]にクランプ済み(0=完全一致、1=無関係、gamut_applyのdにそのまま渡せる)。
fn gamut_distance(cb: f32, cr: f32, key_hue: f32, key_sat: f32, hue_range_turns: f32, sat_range: f32) -> f32 {
    let sat = max(abs(cb), abs(cr));

    let hue = atan2(cr, cb);
    var diff = hue - key_hue;
    diff = diff - TWO_PI * round(diff / TWO_PI);
    let dh_turns = abs(diff) / TWO_PI;

    let hue_excess = 16.0 * max(0.0, dh_turns - hue_range_turns);
    let sat_excess = max(0.0, abs(sat - key_sat) - sat_range);
    return clamp(hue_excess + 8.0 * sat_excess, 0.0, 1.0);
}

// d(=gamut_distanceの結果、または境界補正でフェザリング済みの値)を使って
// Cb/Cr/Yを変換後の色へブレンドする。d=0で完全にafter色、d=1で無変化。
// 輝度は「元の彩度が低い(=無彩色に近い)ピクセルほど輝度変化を抑える」README記載の
// ニュアンスを反映する(key_satに対する相対彩度で減衰させる)。
fn gamut_apply(
    cr: f32, cb: f32, y: f32,
    key_sat: f32, key_y: f32,
    after_cr: f32, after_cb: f32, after_y: f32,
    d: f32,
) -> vec3<f32> {
    let sat = max(abs(cb), abs(cr));
    let out_cr = mix(after_cr, cr, d);
    let out_cb = mix(after_cb, cb, d);

    let luma_t = clamp(max(d, clamp((key_sat - sat) / max(key_sat, 1e-4), 0.0, 1.0)), 0.0, 1.0);
    var target_y = after_y;
    if (key_y > 1e-4 && y < key_y) {
        target_y = after_y * y / key_y;
    }
    let out_y = mix(target_y, y, luma_t);

    return vec3<f32>(out_cr, out_cb, out_y);
}
