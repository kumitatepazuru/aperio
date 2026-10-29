// YUV (BT.709) → RGBA16Float のフルスクリーン三角形パス。
//
// レイアウトは Rust 側から渡すマクロで選ぶ(すべて set 0、binding はこの順):
//   LAYOUT_PLANAR         : Y, Cb, Cr(R8 / R16)                  sampler=3 params=4
//   LAYOUT_PLANAR_ALPHA   : Y, Cb, Cr, A(R8)                     sampler=4 params=5
//   LAYOUT_SEMIPLANAR     : Y, UV(Rg8 / Rg16; R=Cb, G=Cr)         sampler=2 params=3
//   LAYOUT_SEMIPLANAR_VU  : Y, VU(Rg8 / Rg16; R=Cr, G=Cb, NV21)   sampler=2 params=3
// 色差の係数は Cb / Cr が [-0.5, 0.5] に正規化されている前提(YuvConvParams を参照)。

struct YuvParams {
    float y_offset;
    float y_scale;
    float c_offset;
    float c_scale;
};

#if defined(LAYOUT_PLANAR)
[[vk::binding(0, 0)]] Texture2D<float4> y_tex;
[[vk::binding(1, 0)]] Texture2D<float4> cb_tex;
[[vk::binding(2, 0)]] Texture2D<float4> cr_tex;
[[vk::binding(3, 0)]] SamplerState yuv_sampler;
[[vk::binding(4, 0)]] ConstantBuffer<YuvParams> params;
#elif defined(LAYOUT_PLANAR_ALPHA)
[[vk::binding(0, 0)]] Texture2D<float4> y_tex;
[[vk::binding(1, 0)]] Texture2D<float4> cb_tex;
[[vk::binding(2, 0)]] Texture2D<float4> cr_tex;
[[vk::binding(3, 0)]] Texture2D<float4> a_tex;
[[vk::binding(4, 0)]] SamplerState yuv_sampler;
[[vk::binding(5, 0)]] ConstantBuffer<YuvParams> params;
#elif defined(LAYOUT_SEMIPLANAR) || defined(LAYOUT_SEMIPLANAR_VU)
[[vk::binding(0, 0)]] Texture2D<float4> y_tex;
[[vk::binding(1, 0)]] Texture2D<float4> chroma_tex;
[[vk::binding(2, 0)]] SamplerState yuv_sampler;
[[vk::binding(3, 0)]] ConstantBuffer<YuvParams> params;
#else
#error "one of LAYOUT_PLANAR / LAYOUT_PLANAR_ALPHA / LAYOUT_SEMIPLANAR / LAYOUT_SEMIPLANAR_VU must be defined"
#endif

struct VsOut {
    float4 pos : SV_Position;
    [[vk::location(0)]] float2 uv : TEXCOORD0;
};

static const float2 POSITIONS[3] = { float2(-1.0, 1.0), float2(3.0, 1.0), float2(-1.0, -3.0) };
static const float2 UVS[3] = { float2(0.0, 0.0), float2(2.0, 0.0), float2(0.0, 2.0) };

[shader("vertex")]
VsOut vs_main(uint idx : SV_VertexID) {
    VsOut o;
    o.pos = float4(POSITIONS[idx], 0.0, 1.0);
    o.uv = UVS[idx];
    return o;
}

float3 yuv_to_rgb(float y_raw, float cb_raw, float cr_raw) {
    float y = (y_raw - params.y_offset) * params.y_scale;
    float cb = (cb_raw - params.c_offset) * params.c_scale;
    float cr = (cr_raw - params.c_offset) * params.c_scale;
    return clamp(
        float3(
            y + 1.5748 * cr,
            y - 0.18732 * cb - 0.46812 * cr,
            y + 1.8556 * cb),
        0.0, 1.0);
}

[shader("fragment")]
float4 fs_main(VsOut i) : SV_Target {
    float y_raw = y_tex.Sample(yuv_sampler, i.uv).r;
#if defined(LAYOUT_PLANAR)
    float cb_raw = cb_tex.Sample(yuv_sampler, i.uv).r;
    float cr_raw = cr_tex.Sample(yuv_sampler, i.uv).r;
    return float4(yuv_to_rgb(y_raw, cb_raw, cr_raw), 1.0);
#elif defined(LAYOUT_PLANAR_ALPHA)
    float cb_raw = cb_tex.Sample(yuv_sampler, i.uv).r;
    float cr_raw = cr_tex.Sample(yuv_sampler, i.uv).r;
    float a = a_tex.Sample(yuv_sampler, i.uv).r;
    return float4(yuv_to_rgb(y_raw, cb_raw, cr_raw), a);
#elif defined(LAYOUT_SEMIPLANAR)
    float2 uv_raw = chroma_tex.Sample(yuv_sampler, i.uv).rg;
    return float4(yuv_to_rgb(y_raw, uv_raw.r, uv_raw.g), 1.0);
#else
    float2 vu_raw = chroma_tex.Sample(yuv_sampler, i.uv).rg;
    return float4(yuv_to_rgb(y_raw, vu_raw.g, vu_raw.r), 1.0);
#endif
}
