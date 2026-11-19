import aperio_bindings;

struct FigureParams {
    // 0=円, 1=四角形(枠/塗りつぶし), 2=正多角形, 3=星型(五芒星の外形)。
    int shape;
    int out_width;
    int out_height;
    // 多角形の頂点数(三角形3/五角形5/六角形6)。星型は10固定なので無視される。
    int vertices;
    // `サイズ`。円は縦横を `size/out_*` で縮めて箱に収め、多角形は外接円の直径になる
    // (出力が `size` より小さくても縮まず、はみ出た分が切れる)。
    float size;
    // `ライン幅`(px)。円は半径から、多角形は外接円の半径から引く量。
    float line_width;
    // 四角形のみ: 1なら枠(内側 [L, w-L) x [L, h-L) が抜ける)、0なら塗りつぶし。
    int hollow;
};

[vk::binding(0, 1)]
ParameterBlock<EffectResources<FigureParams>> res;

static const float PI = 3.14159265358979;

// 円(楕円): 座標を2倍画素で取り、縦横を size/w・size/h で引き伸ばして「直径2*sizeの円」に
// してから、半径の2乗の残りをそのままランプにする。ランプは縁の内側にある。
float circle_coverage(FigureParams p, int2 coord) {
    let w = float(p.out_width);
    let h = float(p.out_height);
    let s = p.size;
    let x = (2.0 * float(coord.x) - w + 1.0) * s / w;
    let y = (2.0 * float(coord.y) - h + 1.0) * s / h;
    let d2 = x * x + y * y;

    // ランプの幅(半径^2の単位)。size == w == h なら 8*size(半径にして約2画素)。
    let ramp = 8.0 * s * s * s / (w * h);
    var a = clamp((s * s - d2) / ramp, 0.0, 1.0);

    // 内径(2倍画素)。0以下なら穴なし = 塗りつぶし。穴の縁のランプは穴の内側。
    let r = s - 2.0 * p.line_width - 1.0;
    if (r > 0.0) {
        let hole_ramp = 8.0 * s * s * r / (w * h);
        a *= clamp((d2 - r * r + hole_ramp) / hole_ramp, 0.0, 1.0);
    }
    return a;
}

// 四角形・背景: アンチエイリアス無しの2値。
float rectangle_coverage(FigureParams p, int2 coord) {
    if (p.hollow == 0) {
        return 1.0;
    }
    let l = int(p.line_width + 0.5);
    let inside = coord.x >= l && coord.x < p.out_width - l && coord.y >= l && coord.y < p.out_height - l;
    return inside ? 0.0 : 1.0;
}

// 三角形・五角形・六角形・星型: 画素の方位角で辺を1本選び、その辺の半平面からの
// 符号付き距離をランプにする。頂点0は真上で、外接円の中心が画像の中心。
float polygon_coverage(FigureParams p, int2 coord) {
    let is_star = p.shape == 3;
    let n = is_star ? 10 : p.vertices;

    // 2倍画素。縦横の縮尺は掛けない(縦横比は描画側のスケールが受け持つ)。
    let x = 2.0 * float(coord.x) - float(p.out_width) + 1.0;
    let y = 2.0 * float(coord.y) - float(p.out_height) + 1.0;

    // 上から時計回りに測った方位角の扇形。
    let azimuth = atan2(-x, y) + PI;
    var k = int(floor(azimuth * float(n) / (2.0 * PI)));
    k = ((k % n) + n) % n;

    // 辺の法線の角度と、外接円の中心から辺までの距離の係数、`ライン幅` が辺を動かす量の係数。
    var theta: float;
    var edge_cos: float;
    var hole_gain: float;
    if (is_star) {
        // 五芒星の外形: 偶数の扇形は「頂点 j -> j+2」の弦、奇数の扇形は「頂点 j-1 -> j+1」の弦を辺に使う。
        // どちらも中心から弦までの距離は size*cos(2π/5)。
        theta = (k % 2 == 0) ? (2.0 * PI / 5.0 + float(k) * PI / 5.0) : (float(k - 1) * PI / 5.0);
        edge_cos = cos(2.0 * PI / 5.0);
        // 星型だけ cos(2π/5)=0.309017 の代わりに手書きの 2/6.45=0.310078 を使う(線が0.34%太い)。
        hole_gain = 4.0 / 6.45;
    } else {
        theta = 2.0 * PI * float(k) / float(n) + PI / float(n);
        edge_cos = cos(PI / float(n));
        hole_gain = 2.0 * edge_cos;
    }

    // 辺の内側への距離(2倍画素)+2。ランプ幅は4(= 2画素)で、辺を中心に外側と内側へ1画素ずつ。
    let v = p.size * edge_cos + 2.0 - sin(theta) * x + cos(theta) * y;
    var a = clamp(v / 4.0, 0.0, 1.0);

    // `ライン幅` 分だけ外接円を縮めた多角形の内側を抜く。
    let hole_edge = p.line_width * hole_gain + 2.0;
    a *= 1.0 - clamp((v - hole_edge) / 4.0, 0.0, 1.0);
    return a;
}

// 被覆率を .a に書く(rgbは使わない)。色は common/encode_color.slang が載せる。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let coord = int2(global_id.xy);
    let p = res.params[0];

    if (coord.x >= p.out_width || coord.y >= p.out_height) {
        return;
    }

    var a: float;
    if (p.shape == 0) {
        a = circle_coverage(p, coord);
    } else if (p.shape == 1) {
        a = rectangle_coverage(p, coord);
    } else {
        a = polygon_coverage(p, coord);
    }

    res.outputTex[coord] = float4(0.0, 0.0, 0.0, a);
}
