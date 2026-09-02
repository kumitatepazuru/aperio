import aperio_bindings;

struct DiscBlurParams {
    int radius;
    int out_width;
    int out_height;
};

struct InputTextures { Texture2D<float4> tex0; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<DiscBlurParams>> res;

// 半径Rの円板(disc)カーネルによる一様重み平均(exedit-inspect lens_blur README §5)。
// 外周² = R²+R・内周² = R²-Rの間を1画素幅の線形ランプで繋ぎ、円の内側は
// 完全に一様重み1.0になる(ガウシアンでも三角形でもない、box_blurと同じ
// 「窓の形が違うだけ」の一様重みカーネル)。色はアルファ加重(プリマルチプライ)で
// 蓄積してsum_a(加重アルファ和)で正規化し、アルファは別途sum_weight
// (実際にカーネルが覆えた面積の総和)で正規化する ―― この2つの分母が別物である
// 点が肝で、キャンバス外に出たサンプルはどちらの和にも数えない(寄与ゼロ)ため、
// `サイズ固定`のON/OFFに関わらず端でアルファが薄まらない(README §5、
// directional_blurの除数選択とはここが異なる)。curve_forward後のカーブ空間の
// 値(pow(base, x*256)-1で爆発しうる)をそのまま扱うため32bit float必須。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let coord = int2(global_id.xy);

    if (coord.x >= res.params[0].out_width || coord.y >= res.params[0].out_height) {
        return;
    }

    let tex = inputs.tex0;
    uint dimsU, dimsV;
    tex.GetDimensions(dimsU, dimsV);
    let dims = int2(dimsU, dimsV);

    let r = res.params[0].radius;
    let r_safe = max(r, 1);
    let outer2 = r * r + r;
    let inner2 = r * r - r;
    let divisor = float(2 * r_safe);

    var sum_rgb: float3 = float3(0.0, 0.0, 0.0);
    var sum_a: float = 0.0;
    var sum_weight: float = 0.0;

    for (int dy = -r; dy <= r; dy++) {
        for (int dx = -r; dx <= r; dx++) {
            let d2 = dx * dx + dy * dy;
            if (d2 >= outer2) {
                continue;
            }

            let sc = coord + int2(dx, dy);
            if (sc.x < 0 || sc.x >= dims.x || sc.y < 0 || sc.y >= dims.y) {
                continue;
            }

            var weight: float = 1.0;
            if (d2 > inner2) {
                weight = float(outer2 - d2) / divisor;
            }

            let s = tex.Load(int3(sc, 0));
            let a = max(s.a, 0.0);
            sum_rgb += s.rgb * a * weight;
            sum_a += a * weight;
            sum_weight += weight;
        }
    }

    var out_rgb: float3 = float3(0.0, 0.0, 0.0);
    if (sum_a > 1e-6) {
        out_rgb = sum_rgb / sum_a;
    }
    var out_a: float = 0.0;
    if (sum_weight > 1e-6) {
        out_a = sum_a / sum_weight;
    }

    res.outputTex[coord] = float4(out_rgb, out_a);
}
