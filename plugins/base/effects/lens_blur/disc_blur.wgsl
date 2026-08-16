enable wgpu_binding_array;

struct DiscBlurParams {
    radius: i32,
    out_width: i32,
    out_height: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: DiscBlurParams;

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
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= params.out_width || coord.y >= params.out_height) {
        return;
    }

    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));

    let r = params.radius;
    let r_safe = max(r, 1);
    let outer2 = r * r + r;
    let inner2 = r * r - r;
    let divisor = f32(2 * r_safe);

    var sum_rgb = vec3<f32>(0.0);
    var sum_a = 0.0;
    var sum_weight = 0.0;

    for (var dy = -r; dy <= r; dy++) {
        for (var dx = -r; dx <= r; dx++) {
            let d2 = dx * dx + dy * dy;
            if (d2 >= outer2) {
                continue;
            }

            let sc = coord + vec2<i32>(dx, dy);
            if (sc.x < 0 || sc.x >= dims.x || sc.y < 0 || sc.y >= dims.y) {
                continue;
            }

            var weight = 1.0;
            if (d2 > inner2) {
                weight = f32(outer2 - d2) / divisor;
            }

            let s = textureLoad(tex, sc, 0);
            let a = max(s.a, 0.0);
            sum_rgb += s.rgb * a * weight;
            sum_a += a * weight;
            sum_weight += weight;
        }
    }

    var out_rgb = vec3<f32>(0.0);
    if (sum_a > 1e-6) {
        out_rgb = sum_rgb / sum_a;
    }
    var out_a = 0.0;
    if (sum_weight > 1e-6) {
        out_a = sum_a / sum_weight;
    }

    textureStore(outputTex, coord, vec4<f32>(out_rgb, out_a));
}
