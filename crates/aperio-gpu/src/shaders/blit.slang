// テクスチャを別フォーマットのレンダーターゲットへ等倍で写すフルスクリーン三角形パス。
// (共有テクスチャへの書き出しで、作業フォーマット → Rgba16Float / Bgra8Unorm へ変換するのに使う)

[[vk::binding(0, 0)]] Texture2D<float4> src_tex;
[[vk::binding(1, 0)]] SamplerState src_sampler;

struct VsOut {
    float4 pos : SV_Position;
    [[vk::location(0)]] float2 uv : TEXCOORD0;
};

static const float2 POSITIONS[3] = { float2(-1.0, -1.0), float2(3.0, -1.0), float2(-1.0, 3.0) };
// 出力の向きに合わせて縦を反転した UV(NDC の y=-1 が v=1)
static const float2 UVS[3] = { float2(0.0, 1.0), float2(2.0, 1.0), float2(0.0, -1.0) };

[shader("vertex")]
VsOut vs_main(uint idx : SV_VertexID) {
    VsOut o;
    o.pos = float4(POSITIONS[idx], 0.0, 1.0);
    o.uv = UVS[idx];
    return o;
}

[shader("fragment")]
float4 fs_main(VsOut i) : SV_Target {
    return src_tex.Sample(src_sampler, i.uv);
}
