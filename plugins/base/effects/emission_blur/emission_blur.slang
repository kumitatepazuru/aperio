import aperio_bindings;

struct EmissionBlurParams {
    // 拡張後キャンバスの左上が、元オブジェクト座標(未拡張ソースのピクセル添字空間)の
    // どこにあたるか(常に<=0、README §6)。
    int x0;
    int y0;
    int out_width;
    int out_height;
    // 放射の中心(元オブジェクト座標、README §3): cx = w/2 + X, cy = h/2 + Y
    float cx;
    float cy;
    // サンプル数の基準 R'(README §4): (1 - range_frac)*R + R/2
    float r_prime;
    // range_frac = 範囲(UI値)/100。走行距離は常に距離*range_fracになる(README §4)。
    float range_frac;
};

struct InputTextures { Texture2D<float4> tex0; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<EmissionBlurParams>> res;

// 放射ブラーのワーカー(README §1・4・5)。出力画素ごとに中心へ向かう線分を
// サンプルして平均する、凸エッジと同じ「毎画素サンプルを取り直す」素直なループ。
// 密度は3段階(遠方は距離に反比例/近傍1px以内は×2/×4/×8)で切り替わるが、
// どの枝でも走行距離は常に distance*range_frac で一定になる(README §4)。
//
// 実機は距離を8倍した固定小数点(d8)で分岐・サンプル数を決め、0x10624dd3の
// マジック定数除算で割り算を再現しているが、ここではその意図(密度切り替えの
// 閾値と歩幅の縮尺)だけを浮動小数点でそのまま再現し、固定小数点化やtrunc丸めは
// 行わない(docs/plugin.md #1)。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let out_coord = int2(global_id.xy);
    if (out_coord.x >= res.params[0].out_width || out_coord.y >= res.params[0].out_height) {
        return;
    }

    let src = inputs.tex0;
    uint dimsU, dimsV;
    src.GetDimensions(dimsU, dimsV);
    let src_dims = int2(dimsU, dimsV);

    // 元オブジェクト座標(未拡張ソースのピクセル添字空間)へ戻す
    let x = out_coord.x + res.params[0].x0;
    let y = out_coord.y + res.params[0].y0;

    let dx = res.params[0].cx - float(x);
    let dy = res.params[0].cy - float(y);
    let dist = length(float2(dx, dy));

    // d8相当(距離を8倍した量)。実機はここをi32へtruncするが、ここでは浮動小数点の
    // まま扱う(密度切り替えの閾値8/4/2はREADME §4の表のまま)。
    let dd = dist * 8.0;

    var d_final: float;
    var n: float;
    if (dd > res.params[0].r_prime) {
        // 遠い(README §4表): サンプル数はR'・range_fracで頭打ちになり距離に
        // よらず一定、歩幅はdx/R'(中心までの距離が伸びるほど粗くなる)。
        d_final = res.params[0].r_prime;
        n = res.params[0].r_prime * res.params[0].range_frac;
    } else {
        // 近い: 中心から離れるにつれ ×2 → ×4 → ×8 と密度を上げる
        // (dd<=2は下のpass-through判定で必ず弾かれるのでm=1のままでよい)。
        var m: float = 1.0;
        if (dd > 8.0) {
            m = 8.0;
        } else if (dd > 4.0) {
            m = 4.0;
        } else if (dd > 2.0) {
            m = 2.0;
        }
        d_final = dd * m;
        n = res.params[0].range_frac * dd * m;
    }

    var out_color: float4 = float4(0.0, 0.0, 0.0, 0.0);

    if (d_final < 2.0 || n < 2.0) {
        // README §4: 中心のごく近傍(中心そのものを含む)は平均せず素通し(コピー)。
        if (x >= 0 && x < src_dims.x && y >= 0 && y < src_dims.y) {
            out_color = src.Load(int3(x, y, 0));
        }
    } else {
        let n_i = int(n);
        let step = float2(dx, dy) / d_final;
        // 自分自身を第0サンプルとして中心方向へn_i個(README §4末尾)。
        let start = float2(float(x), float(y)) + 0.5;

        // README §5: 全オブジェクト効果共通のプリマルチプライド累積(box_blur.md §1)。
        // ただしサンプル数・向きが画素ごとに違うため、共通のbox_sum(aperio-blur.slang)には
        // 乗らず専用ループになる(凸エッジ・閃光と同じ事情)。
        var sum_rgb: float3 = float3(0.0, 0.0, 0.0);
        var sum_a: float = 0.0;
        for (int i = 0; i < n_i; i++) {
            let p = start + step * float(i);
            let c = int2(floor(p));
            if (c.x >= 0 && c.x < src_dims.x && c.y >= 0 && c.y < src_dims.y) {
                let s = src.Load(int3(c, 0));
                sum_rgb += s.rgb * s.a;
                sum_a += s.a;
            }
            // 範囲外のサンプルは寄与ゼロ(透明として数える)。ただし下のn_iに
            // よる除算には常に数える(README §5: 実際に画像に当たった数ではない)。
        }

        var out_rgb: float3 = float3(0.0, 0.0, 0.0);
        if (sum_a > 0.0) {
            out_rgb = sum_rgb / sum_a;
        }
        // アルファの除数は生のサンプル数n(実際に画像に当たった数ではない、README §5)。
        // fixed_sizeはキャンバス矩形だけに効き、この式自体は変えない(README §6)。
        out_color = float4(out_rgb, sum_a / float(n_i));
    }

    res.outputTex[out_coord] = out_color;
}
