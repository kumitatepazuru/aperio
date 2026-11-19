// ブレンドモード用のヘルパーモジュール。
//
// 最小構成のブレンドモードは次の通り(blend_rgb を差し替えるだけでよい):
//
//   import aperio_blend;
//
//   [shader("vertex")]
//   BlendVaryings vs_main(uint vertex_id : SV_VertexID) { return blend_vertex(vertex_id); }
//
//   [shader("fragment")]
//   float4 fs_main(BlendVaryings v) : SV_Target {
//       BlendFragment f;
//       if (!blend_fetch(v, f)) discard;
//       let blend_rgb = f.src.rgb;           // ← モードごとの合成色
//       return blend_composite(f, blend_rgb);
//   }
module aperio_blend;

// レイヤーの配置パラメーター(frame_builder.blend_layer_params と同じ並び、64バイト)。
public struct BlendLayerParams {
    // レイヤーのローカル座標(基点を原点とするpx) → キャンバス中央を原点とするpx への
    // 3x3ホモグラフィ(拡大率・3D回転・平行移動・透視投影を畳み込んだもの)の各行。wは未使用。
    public float4 transform_row0;
    public float4 transform_row1;
    public float4 transform_row2;
    // 回転・拡縮の基点オフセット(レイヤーテクスチャ中心からのpx)
    public float2 center;
    // レイヤーの不透明度(0.0〜1.0)。0なら何も描かない
    public float alpha;
    public float _pad;
};

// set 0: 入力テクスチャ。binding 0 = 編集中のフレーム、binding 1 = 合成するレイヤー
public struct BlendInputs {
    public Texture2D<float4> dst;
    public Texture2D<float4> src;
};

// set 1: サンプラー(レイヤーのサンプリング用) → 配置パラメーター
public struct BlendResources {
    public SamplerState samp;
    public StructuredBuffer<BlendLayerParams> params;
};

[vk::binding(0, 0)]
public ParameterBlock<BlendInputs> blend_inputs;
[vk::binding(0, 1)]
public ParameterBlock<BlendResources> blend_res;

// レイヤー矩形を外側へ広げる量(px)。辺上の画素の取りこぼしを防ぐためで、はみ出した分は
// blend_fetch の範囲判定で捨てられる。
static const float BLEND_EDGE_MARGIN = 1.0;
// 透視投影の w がこれ未満になる(カメラより手前に来る)部分はクリップする。
static const float BLEND_MIN_W = 1e-5;

public struct BlendVaryings {
    public float4 position : SV_Position;
    // レイヤーテクスチャ上のpx座標(透視補正つきで補間される)
    public float2 src_px : TEXCOORD0;
};

// フラグメントの合成材料
public struct BlendFragment {
    // 編集中のフレームの、この画素の色
    public float4 dst;
    // レイヤーをこの画素へ配置してサンプルした色(straight alpha)
    public float4 src;
    // 合成に使う不透明度 = src.a * レイヤーの不透明度
    public float alpha;
};

public float2 blend_src_size() {
    uint w, h;
    blend_inputs.src.GetDimensions(w, h);
    return float2(w, h);
}

public float2 blend_dst_size() {
    uint w, h;
    blend_inputs.dst.GetDimensions(w, h);
    return float2(w, h);
}

// レイヤー矩形(2枚の三角形 = 6頂点)を配置する頂点シェーダー本体。
// vertex_id には SV_VertexID をそのまま渡す。
public BlendVaryings blend_vertex(uint vertex_id) {
    // StructuredBufferの構造体をまるごとコピーせず、フィールドごとに読む
    let row0 = blend_res.params[0].transform_row0.xyz;
    let row1 = blend_res.params[0].transform_row1.xyz;
    let row2 = blend_res.params[0].transform_row2.xyz;
    let center = blend_res.params[0].center;
    let src_size = blend_src_size();
    let dst_size = blend_dst_size();

    // (0,0) (1,0) (0,1) / (0,1) (1,0) (1,1)
    let corner = vertex_id % 6;
    let t = float2(
        (corner == 1 || corner == 4 || corner == 5) ? 1.0 : 0.0,
        (corner == 2 || corner == 3 || corner == 5) ? 1.0 : 0.0);
    let src_px = lerp(float2(-BLEND_EDGE_MARGIN), src_size + BLEND_EDGE_MARGIN, t);

    let local = float3(src_px - src_size * 0.5 - center, 1.0);
    let q = float3(dot(row0, local), dot(row1, local), dot(row2, local));

    // キャンバス中央原点のpx (q.xy / q.z) をNDCへ(y+が上)。w = q.z を掛けた形のまま出力すると
    // ハードウェアの透視除算で元に戻り、src_px も透視補正つきで補間される。
    // z = w - BLEND_MIN_W なので、深度クリップ(0 <= z)が w >= BLEND_MIN_W の面で切ってくれる。
    BlendVaryings o;
    o.position = float4(q.x * 2.0 / dst_size.x, -q.y * 2.0 / dst_size.y, q.z - BLEND_MIN_W, q.z);
    o.src_px = src_px;
    return o;
}

// この画素の合成材料を読む。レイヤーの外、または完全に透明なら false (呼び出し側で discard する)。
public bool blend_fetch(BlendVaryings v, out BlendFragment f) {
    f.dst = blend_inputs.dst.Load(int3(int2(v.position.xy), 0));
    f.src = float4(0.0);
    f.alpha = 0.0;

    let src_size = blend_src_size();
    if (v.src_px.x < 0.0 || v.src_px.y < 0.0 || v.src_px.x >= src_size.x || v.src_px.y >= src_size.y) {
        return false;
    }

    f.src = blend_inputs.src.SampleLevel(blend_res.samp, v.src_px / src_size, 0.0);
    f.alpha = saturate(f.src.a * blend_res.params[0].alpha);
    return f.alpha > 0.0;
}

// blend_rgb をこの画素の色として、フレームへ通常合成(Porter-Duff の over)する。
// 全ブレンドモード共通の末尾で、モードごとに違うのは blend_rgb の求め方だけ:
//   out_a = a_s + a_d * (1 - a_s)
//   out_c = (c_d * a_d * (1 - a_s) + blend_c * a_s) / out_a
public float4 blend_composite(BlendFragment f, float3 blend_rgb) {
    let a_s = f.alpha;
    let a_d = saturate(f.dst.a);
    let out_a = a_s + a_d * (1.0 - a_s);
    if (out_a <= 0.0) {
        return float4(0.0);
    }
    let out_rgb = (f.dst.rgb * a_d * (1.0 - a_s) + blend_rgb * a_s) / out_a;
    return float4(out_rgb, out_a);
}
