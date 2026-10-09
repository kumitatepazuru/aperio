import aperio_math;
import aperio_bindings;

struct DirectionalBlurParams {
    // 拡張後キャンバスの左上が、元オブジェクト座標(未拡張ソースのピクセル添字空間)の
    // どこにあたるか(常に<=0、README §7)。
    int x0;
    int y0;
    int out_width;
    int out_height;
    // 単位方向ベクトル(README §4: (-sinθ, +cosθ)。符号は出力に影響しない)。
    float dir_x;
    float dir_y;
    // サンプル歩幅[px](README §5の4分岐で決まる)。
    float step;
    // 片側サンプル数N(README §5、N*step ≈ range)。
    int n;
    // 0: サイズ固定OFF ― 常にカーネル幅2N+1で割る(枠外を透明として数える→端でフェードする)
    // 1: サイズ固定ON  ― 枠内サンプルの本数で割る(端で正規化し直す→減衰しない、README §6)
    int fixed_size;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<DirectionalBlurParams>> res;

// README §1: 出力画素を中心に角度方向へ±range画素の線分を取り、その上を等間隔に
// サンプルしてアルファ加重(プリマルチプライ)平均する。窓が画素によらず一定方向
// なので分離型ボックス平均にできそうに見えるが、斜め方向では窓が行に沿って平行移動
// しないため差分更新はできず(box_blur.md §3/凸エッジと同じ事情)、毎画素サンプルを
// 取り直す。glint.slang と同様、拡張後キャンバスの座標系のまま元の(未拡張)ソース
// テクスチャを直接オフセット付きで読む1発シェーダーとし、expand.slangの別パスは
// 使わない。
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
    let src_dims = float2(dimsU, dimsV);

    // 元オブジェクト座標(未拡張ソースのピクセル添字空間)へ戻す
    let base = float2(out_coord + int2(res.params[0].x0, res.params[0].y0));
    let dir = float2(res.params[0].dir_x, res.params[0].dir_y);

    var sum_rgb: float3 = float3(0.0, 0.0, 0.0);
    var sum_a: float = 0.0;
    var valid_count: int = 0;

    for (int k = -res.params[0].n; k <= res.params[0].n; k++) {
        let p = base + dir * (res.params[0].step * float(k));
        // README §6: 連続座標での境界判定(元の未拡張ソースの矩形基準)。範囲外は
        // 寄与ゼロ(ゼロパディング、box_blur.md border_mode=1相当)。
        if (p.x >= 0.0 && p.x < src_dims.x && p.y >= 0.0 && p.y < src_dims.y) {
            let s = bilinear4_load(src, p);
            sum_rgb += s.rgb * s.a;
            sum_a += s.a;
            valid_count++;
        }
    }

    var divisor: float;
    if (res.params[0].fixed_size != 0) {
        divisor = float(max(valid_count, 1));
    } else {
        divisor = float(2 * res.params[0].n + 1);
    }

    var out_rgb: float3 = float3(0.0, 0.0, 0.0);
    if (sum_a > 0.0) {
        out_rgb = sum_rgb / sum_a;
    }
    let out_a = sum_a / divisor;

    res.outputTex[out_coord] = float4(out_rgb, out_a);
}
