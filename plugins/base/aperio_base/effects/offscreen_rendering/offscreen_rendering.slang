import aperio_bindings;

// TODO: combine.slangを使うようにする？
struct OffscreenParams {
    float4 inv_col0;   // 逆ホモグラフィの列0
    float4 inv_col1;   // 同・列1
    float4 inv_col2;   // 同・列2
    float center_x;    // 回転・拡縮の基点オフセットX (入力テクスチャ中心からのピクセル数)
    float center_y;    // 同・Y
    float alpha;       // 透明度 (0.0〜1.0)
    float _pad;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResourcesSampler<OffscreenParams>> res;

[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let out_coord = int2(global_id.xy);
    uint out_w, out_h;
    res.outputTex.GetDimensions(out_w, out_h);
    if (out_coord.x >= int(out_w) || out_coord.y >= int(out_h)) {
        return;
    }

    let inv_col0 = res.params[0].inv_col0;
    let inv_col1 = res.params[0].inv_col1;
    let inv_col2 = res.params[0].inv_col2;
    let center = float2(res.params[0].center_x, res.params[0].center_y);
    let alpha = res.params[0].alpha;

    let tex = inputs.tex0;
    uint src_w, src_h;
    tex.GetDimensions(src_w, src_h);
    let src_dims = float2(src_w, src_h);

    // 出力の中央が原点。+0.5 はピクセル中心を指す。
    let relative_coord = float2(out_coord) + float2(0.5, 0.5) - float2(out_w, out_h) * 0.5;
    let q = inv_col0.xyz * relative_coord.x + inv_col1.xyz * relative_coord.y + inv_col2.xyz;
    if (alpha <= 0.0 || q.z <= 0.0) {
        res.outputTex[out_coord] = float4(0.0, 0.0, 0.0, 0.0);
        return;
    }

    let src_pixel = q.xy / q.z + src_dims * 0.5 + center;
    if (src_pixel.x < 0.0 || src_pixel.x >= src_dims.x || src_pixel.y < 0.0 || src_pixel.y >= src_dims.y) {
        // フレームの外・オブジェクトの外は透明(フレーム大の透明バッファへ描く)
        res.outputTex[out_coord] = float4(0.0, 0.0, 0.0, 0.0);
        return;
    }

    let color = tex.SampleLevel(res.samp, src_pixel / src_dims, 0.0);
    res.outputTex[out_coord] = float4(color.rgb, color.a * alpha);
}
