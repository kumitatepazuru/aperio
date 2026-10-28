// グリフアトラスから文字をインスタンシング描画するシェーダー。
//
// アトラスのフォーマットによって 2 つのフラグメントエントリポイントを使い分ける:
//   fs_mask  … R8Unorm アトラス(マスクグリフ): .r チャンネルが coverage
//   fs_color … Rgba8Unorm アトラス(カラーグリフ): .rgba がそのままグリフ色

struct Uniforms {
    float2 output_size;
    float2 _pad;
};

[[vk::binding(0, 0)]] Texture2D<float4> t_atlas;
[[vk::binding(1, 0)]] SamplerState s_atlas;
[[vk::binding(2, 0)]] ConstantBuffer<Uniforms> uniforms;

struct VertexOut {
    float4 pos : SV_Position;
    [[vk::location(0)]] float2 uv : TEXCOORD0;
    [[vk::location(1)]] float4 color : COLOR0;
};

static const float2 CORNERS[6] = {
    float2(0.0, 0.0),
    float2(1.0, 0.0),
    float2(0.0, 1.0),
    float2(1.0, 0.0),
    float2(1.0, 1.0),
    float2(0.0, 1.0),
};

[shader("vertex")]
VertexOut vs_main(
    uint vi : SV_VertexID,
    // インスタンスデータ(頂点バッファ 0、インスタンスごと)
    [[vk::location(0)]] float2 inst_pos : INST_POS,       // 出力テクスチャ上のピクセル位置(左上)
    [[vk::location(1)]] float2 inst_size : INST_SIZE,     // グリフのピクセルサイズ
    [[vk::location(2)]] float2 inst_uv_min : INST_UV_MIN, // アトラス UV 左上(0-1 正規化)
    [[vk::location(3)]] float2 inst_uv_max : INST_UV_MAX, // アトラス UV 右下(0-1 正規化)
    [[vk::location(4)]] float4 inst_color : INST_COLOR)   // RGBA テキストカラー(0-1 正規化)
{
    // 2 三角形で四角形を描く(TRIANGLE_LIST, 6 頂点)
    float2 c = CORNERS[vi];
    float2 pixel = inst_pos + c * inst_size;
    // ピクセル座標 → NDC(Y 軸を反転)
    float2 ndc = float2(
        pixel.x / uniforms.output_size.x * 2.0 - 1.0,
        1.0 - pixel.y / uniforms.output_size.y * 2.0);

    VertexOut o;
    o.pos = float4(ndc, 0.0, 1.0);
    o.uv = lerp(inst_uv_min, inst_uv_max, c);
    o.color = inst_color;
    return o;
}

// ── マスクグリフ (R8Unorm アトラス) ──────────────────────────────────────────
// r = coverage(グリフの被覆率)、color = 指定テキストカラー。
[shader("fragment")]
float4 fs_mask(VertexOut i) : SV_Target {
    float coverage = t_atlas.Sample(s_atlas, i.uv).r;
    return float4(i.color.rgb, coverage * i.color.a);
}

// ── カラーグリフ (Rgba8Unorm アトラス) ───────────────────────────────────────
// s.rgba = 絵文字本来の色、color = (1,1,1,alpha) で不透明度のみ制御。
// 統一式: output.rgb = s.rgb * color.rgb、output.a = s.a * color.a
[shader("fragment")]
float4 fs_color(VertexOut i) : SV_Target {
    float4 s = t_atlas.Sample(s_atlas, i.uv);
    return float4(s.rgb * i.color.rgb, s.a * i.color.a);
}
