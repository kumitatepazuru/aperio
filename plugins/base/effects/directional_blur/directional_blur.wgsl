enable wgpu_binding_array;

#import aperio::math::{bilinear4_load}

struct DirectionalBlurParams {
    // 拡張後キャンバスの左上が、元オブジェクト座標(未拡張ソースのピクセル添字空間)の
    // どこにあたるか(常に<=0、README §7)。
    x0: i32,
    y0: i32,
    out_width: i32,
    out_height: i32,
    // 単位方向ベクトル(README §4: (-sinθ, +cosθ)。符号は出力に影響しない)。
    dir_x: f32,
    dir_y: f32,
    // サンプル歩幅[px](README §5の4分岐で決まる)。
    step: f32,
    // 片側サンプル数N(README §5、N*step ≈ range)。
    n: i32,
    // 0: サイズ固定OFF ― 常にカーネル幅2N+1で割る(枠外を透明として数える→端でフェードする)
    // 1: サイズ固定ON  ― 枠内サンプルの本数で割る(端で正規化し直す→減衰しない、README §6)
    fixed_size: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: DirectionalBlurParams;

// README §1: 出力画素を中心に角度方向へ±range画素の線分を取り、その上を等間隔に
// サンプルしてアルファ加重(プリマルチプライ)平均する。窓が画素によらず一定方向
// なので分離型ボックス平均にできそうに見えるが、斜め方向では窓が行に沿って平行移動
// しないため差分更新はできず(box_blur.md §3/凸エッジと同じ事情)、毎画素サンプルを
// 取り直す。glint.wgsl と同様、拡張後キャンバスの座標系のまま元の(未拡張)ソース
// テクスチャを直接オフセット付きで読む1発シェーダーとし、expand.wgslの別パスは
// 使わない。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let out_coord = vec2<i32>(global_id.xy);
    if (out_coord.x >= params.out_width || out_coord.y >= params.out_height) {
        return;
    }

    let src = inputTex[0];
    let src_dims = vec2<f32>(textureDimensions(src));

    // 元オブジェクト座標(未拡張ソースのピクセル添字空間)へ戻す
    let base = vec2<f32>(out_coord + vec2<i32>(params.x0, params.y0));
    let dir = vec2<f32>(params.dir_x, params.dir_y);

    var sum_rgb = vec3<f32>(0.0);
    var sum_a = 0.0;
    var valid_count = 0;

    for (var k = -params.n; k <= params.n; k++) {
        let p = base + dir * (params.step * f32(k));
        // README §6: 連続座標での境界判定(元の未拡張ソースの矩形基準)。範囲外は
        // 寄与ゼロ(ゼロパディング、box_blur.md border_mode=1相当)。
        if (p.x >= 0.0 && p.x < src_dims.x && p.y >= 0.0 && p.y < src_dims.y) {
            let s = bilinear4_load(src, p);
            sum_rgb += s.rgb * s.a;
            sum_a += s.a;
            valid_count++;
        }
    }

    var divisor: f32;
    if (params.fixed_size != 0) {
        divisor = f32(max(valid_count, 1));
    } else {
        divisor = f32(2 * params.n + 1);
    }

    var out_rgb = vec3<f32>(0.0);
    if (sum_a > 0.0) {
        out_rgb = sum_rgb / sum_a;
    }
    let out_a = sum_a / divisor;

    textureStore(outputTex, out_coord, vec4<f32>(out_rgb, out_a));
}
