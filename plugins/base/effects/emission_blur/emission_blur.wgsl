enable wgpu_binding_array;

struct EmissionBlurParams {
    // 拡張後キャンバスの左上が、元オブジェクト座標(未拡張ソースのピクセル添字空間)の
    // どこにあたるか(常に<=0、README §6)。
    x0: i32,
    y0: i32,
    out_width: i32,
    out_height: i32,
    // 放射の中心(元オブジェクト座標、README §3): cx = w/2 + X, cy = h/2 + Y
    cx: f32,
    cy: f32,
    // サンプル数の基準 R'(README §4): (1 - range_frac)*R + R/2
    r_prime: f32,
    // range_frac = 範囲(UI値)/100。走行距離は常に距離*range_fracになる(README §4)。
    range_frac: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: EmissionBlurParams;

// 放射ブラーのワーカー(README §1・4・5)。出力画素ごとに中心へ向かう線分を
// サンプルして平均する、凸エッジと同じ「毎画素サンプルを取り直す」素直なループ。
// 密度は3段階(遠方は距離に反比例/近傍1px以内は×2/×4/×8)で切り替わるが、
// どの枝でも走行距離は常に distance*range_frac で一定になる(README §4)。
//
// 実機は距離を8倍した固定小数点(d8)で分岐・サンプル数を決め、0x10624dd3の
// マジック定数除算で割り算を再現しているが、ここではその意図(密度切り替えの
// 閾値と歩幅の縮尺)だけを浮動小数点でそのまま再現し、固定小数点化やtrunc丸めは
// 行わない(docs/plugin.md #1)。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let out_coord = vec2<i32>(global_id.xy);
    if (out_coord.x >= params.out_width || out_coord.y >= params.out_height) {
        return;
    }

    let src = inputTex[0];
    let src_dims = vec2<i32>(textureDimensions(src));

    // 元オブジェクト座標(未拡張ソースのピクセル添字空間)へ戻す
    let x = out_coord.x + params.x0;
    let y = out_coord.y + params.y0;

    let dx = params.cx - f32(x);
    let dy = params.cy - f32(y);
    let dist = length(vec2<f32>(dx, dy));

    // d8相当(距離を8倍した量)。実機はここをi32へtruncするが、ここでは浮動小数点の
    // まま扱う(密度切り替えの閾値8/4/2はREADME §4の表のまま)。
    let dd = dist * 8.0;

    var d_final: f32;
    var n: f32;
    if (dd > params.r_prime) {
        // 遠い(README §4表): サンプル数はR'・range_fracで頭打ちになり距離に
        // よらず一定、歩幅はdx/R'(中心までの距離が伸びるほど粗くなる)。
        d_final = params.r_prime;
        n = params.r_prime * params.range_frac;
    } else {
        // 近い: 中心から離れるにつれ ×2 → ×4 → ×8 と密度を上げる
        // (dd<=2は下のpass-through判定で必ず弾かれるのでm=1のままでよい)。
        var m = 1.0;
        if (dd > 8.0) {
            m = 8.0;
        } else if (dd > 4.0) {
            m = 4.0;
        } else if (dd > 2.0) {
            m = 2.0;
        }
        d_final = dd * m;
        n = params.range_frac * dd * m;
    }

    var out_color = vec4<f32>(0.0);

    if (d_final < 2.0 || n < 2.0) {
        // README §4: 中心のごく近傍(中心そのものを含む)は平均せず素通し(コピー)。
        if (x >= 0 && x < src_dims.x && y >= 0 && y < src_dims.y) {
            out_color = textureLoad(src, vec2<i32>(x, y), 0);
        }
    } else {
        let n_i = i32(n);
        let step = vec2<f32>(dx, dy) / d_final;
        // 自分自身を第0サンプルとして中心方向へn_i個(README §4末尾)。
        let start = vec2<f32>(f32(x), f32(y)) + 0.5;

        // README §5: 全オブジェクト効果共通のプリマルチプライド累積(box_blur.md §1)。
        // ただしサンプル数・向きが画素ごとに違うため、共通のbox_sum(blur.wgsl)には
        // 乗らず専用ループになる(凸エッジ・閃光と同じ事情)。
        var sum_rgb = vec3<f32>(0.0);
        var sum_a = 0.0;
        for (var i = 0; i < n_i; i++) {
            let p = start + step * f32(i);
            let c = vec2<i32>(floor(p));
            if (c.x >= 0 && c.x < src_dims.x && c.y >= 0 && c.y < src_dims.y) {
                let s = textureLoad(src, c, 0);
                sum_rgb += s.rgb * s.a;
                sum_a += s.a;
            }
            // 範囲外のサンプルは寄与ゼロ(透明として数える)。ただし下のn_iに
            // よる除算には常に数える(README §5: 実際に画像に当たった数ではない)。
        }

        var out_rgb = vec3<f32>(0.0);
        if (sum_a > 0.0) {
            out_rgb = sum_rgb / sum_a;
        }
        // アルファの除数は生のサンプル数n(実際に画像に当たった数ではない、README §5)。
        // fixed_sizeはキャンバス矩形だけに効き、この式自体は変えない(README §6)。
        out_color = vec4<f32>(out_rgb, sum_a / f32(n_i));
    }

    textureStore(outputTex, out_coord, out_color);
}
