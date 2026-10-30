import aperio_bindings;

struct ResizeDownParams {
    int src_width;
    int src_height;
    int out_width;
    int out_height;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<ResizeDownParams>> res;

// アルファ加重の面積平均によるダウンサンプル(exedit-inspect lens_blur README §4
// 「(4)/(7) リサイズ」)。出力画素ごとに対応する入力矩形(scale = src/dst)を求め、
// その矩形に「中心」が収まる入力テクセルだけを単純平均する ―― 部分被覆の面積
// 按分(サブピクセルカバレッジ)までは行わないニアレストネイバー矩形サンプリング
// による簡略化。curve_forward後のカーブ空間の値をそのまま扱うため、このシェーダー
// は32bit float必須(disc_blur.slangと同じ理由)。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let out_coord = int2(global_id.xy);

    if (out_coord.x >= res.params[0].out_width || out_coord.y >= res.params[0].out_height) {
        return;
    }

    let tex = inputs.tex0;
    let src_dims = int2(res.params[0].src_width, res.params[0].src_height);
    let scale = float2(src_dims) / float2(float(res.params[0].out_width), float(res.params[0].out_height));

    let rect_lo = float2(out_coord) * scale;
    let rect_hi = float2(out_coord + int2(1, 1)) * scale;

    let ix_lo = clamp(int(floor(rect_lo.x)), 0, src_dims.x - 1);
    let ix_hi = clamp(int(ceil(rect_hi.x)), 0, src_dims.x - 1);
    let iy_lo = clamp(int(floor(rect_lo.y)), 0, src_dims.y - 1);
    let iy_hi = clamp(int(ceil(rect_hi.y)), 0, src_dims.y - 1);

    var sum_rgb: float3 = float3(0.0, 0.0, 0.0);
    var sum_a: float = 0.0;
    var count: int = 0;

    for (int sy = iy_lo; sy <= iy_hi; sy++) {
        for (int sx = ix_lo; sx <= ix_hi; sx++) {
            let center = float2(float(sx) + 0.5, float(sy) + 0.5);
            if (center.x >= rect_lo.x && center.x < rect_hi.x && center.y >= rect_lo.y && center.y < rect_hi.y) {
                let s = tex.Load(int3(sx, sy, 0));
                let a = max(s.a, 0.0);
                sum_rgb += s.rgb * a;
                sum_a += a;
                count++;
            }
        }
    }

    if (count == 0) {
        // 矩形が1テクセルの中心すら含まない縮退ケース(通常は起きないが安全策
        // として矩形中心に最も近いテクセルへフォールバックする)。
        let fx = clamp(int(floor((rect_lo.x + rect_hi.x) * 0.5)), 0, src_dims.x - 1);
        let fy = clamp(int(floor((rect_lo.y + rect_hi.y) * 0.5)), 0, src_dims.y - 1);
        res.outputTex[out_coord] = tex.Load(int3(fx, fy, 0));
        return;
    }

    var out_rgb: float3 = float3(0.0, 0.0, 0.0);
    if (sum_a > 1e-6) {
        out_rgb = sum_rgb / sum_a;
    }
    let out_a = sum_a / float(count);

    res.outputTex[out_coord] = float4(out_rgb, out_a);
}
