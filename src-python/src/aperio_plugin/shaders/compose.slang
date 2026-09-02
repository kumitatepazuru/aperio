// TODO: fragment shaderなどをstepで扱えるようにして、簡略化・高速化をする

// 各レイヤーのメタ情報を格納する構造体。
// inv_transformはWGSL時代のmat3x3<f32>(列優先、各列16バイトパディング)と
// 同一のメモリレイアウトになるよう列ごとにfloat4で明示しておく
// (Slang/HLSL側のfloat3x3の既定パッキング規則に依存しない、frame_builder.pyの
// 生バイト列と確実に一致させるための選択)。q = col0*x + col1*y + col2*1 が
// 同次座標での逆ホモグラフィ適用に相当する。
struct LayerParams {
  float4 inv_col0;              // 3D回転・拡大率・平行移動・透視投影を畳み込んだ逆ホモグラフィの列0
  float4 inv_col1;              // 同・列1
  float4 inv_col2;              // 同・列2
  float center_x;               // 回転・拡縮の基点オフセットX (レイヤーテクスチャ中心からのピクセル数)
  float center_y;                // 回転・拡縮の基点オフセットY (レイヤーテクスチャ中心からのピクセル数)
  float alpha;                   // レイヤーの透明度 (0.0〜1.0)
  float _pad;                    // アライメント用パディング
};

// --- リソースのバインディング定義 ---
// 入力テクスチャは真の可変長(レイヤー数はフレームごとに変わる)なので、
// group0は配列1つのまま。出力・サンプラー・paramsはgroup1にまとめる。

// グループ0: テクスチャ関連(可変長)
struct InputTextures {
    Texture2D<float4> tex[];
};
ParameterBlock<InputTextures> inputs;

// グループ1: 出力・サンプラー・メタデータ
struct EffectResources {
    [vk::image_format(APERIO_IMAGE_FORMAT)]
    RWTexture2D<float4> outputTex;
    SamplerState linear_sampler;
    StructuredBuffer<LayerParams> layer_params_array;
};
ParameterBlock<EffectResources> res;

// --- コンピュートシェーダー本体 ---

[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
  let output_coord = int2(global_id.xy);
  uint output_w, output_h;
  res.outputTex.GetDimensions(output_w, output_h);

  // 処理対象が出力テクスチャの範囲外であれば、何もしない
  if (output_coord.x >= int(output_w) || output_coord.y >= int(output_h)) {
    return;
  }

  // このピクセルの最終的な色。初期値は黒 (背景)
  var final_color = float4(0.0, 0.0, 0.0, 255.0);

  uint num_layers, layer_stride;
  res.layer_params_array.GetDimensions(num_layers, layer_stride);

  // 全てのレイヤーを順番に重ね合わせる
  for (uint i = 0; i < num_layers; i++) {
    // StructuredBufferから構造体をまるごとローカル変数へコピーすると(`let params =
    // layer_params_array[i];`)、バッファ側の物理レイアウト型とローカルの論理型が
    // SPIR-V上別の型になり`OpCopyLogical`が生成される。wgpu(naga)はこの命令を
    // サポートしないため、フィールドごとに個別ロードする。
    let inv_col0 = res.layer_params_array[i].inv_col0;
    let inv_col1 = res.layer_params_array[i].inv_col1;
    let inv_col2 = res.layer_params_array[i].inv_col2;
    let center_x = res.layer_params_array[i].center_x;
    let center_y = res.layer_params_array[i].center_y;
    let layer_alpha = res.layer_params_array[i].alpha;
    Texture2D<float4> layer_tex = inputs.tex[i];
    uint layer_w, layer_h;
    layer_tex.GetDimensions(layer_w, layer_h);
    let layer_dims_f = float2(layer_w, layer_h);
    if (layer_alpha <= 0.0) {
      continue;
    }

    // 出力ピクセル座標から、レイヤーテクスチャ上の対応する座標を計算する。
    // 出力テクスチャの中央が原点。+0.5 はピクセル中心を指すため
    // (後段の src_coord_pixel / layer_dims_f がテクセル中心と一致するようになる)。
    let output_dims_f = float2(output_w, output_h);
    let output_center = output_dims_f * 0.5;
    let relative_coord = float2(output_coord) + float2(0.5, 0.5) - output_center;

    // 事前に計算された逆ホモグラフィ (3D回転 + 拡縮 + 平行移動 + 透視投影) を適用
    let q = inv_col0.xyz * relative_coord.x + inv_col1.xyz * relative_coord.y + inv_col2.xyz;
    if (q.z <= 0.0) {
      continue;  // カメラ平面より後方 (このレイヤーには映らない画素)
    }
    let src_rel = q.xy / q.z;

    // 基点オフセットを加えてテクスチャピクセル座標に変換
    let src_coord_pixel = src_rel + layer_dims_f * 0.5 + float2(center_x, center_y);

    if (src_coord_pixel.x >= 0.0 && src_coord_pixel.x < layer_dims_f.x &&
        src_coord_pixel.y >= 0.0 && src_coord_pixel.y < layer_dims_f.y) {

      // サンプルするために座標を正規化
      let src_coord_normalized = src_coord_pixel / layer_dims_f;
      let src_color = layer_tex.SampleLevel(res.linear_sampler, src_coord_normalized, 0.0);

      // --- アルファブレンディング (Over演算) ---
      // 現在の色 (destination color) の上に新しいレイヤーの色を重ねる
      let dst_color = final_color;
      let alpha = src_color.a * layer_alpha;

      let blended_rgb = src_color.rgb * alpha + dst_color.rgb * (1.0 - alpha);
      let blended_a = src_color.a + dst_color.a * (1.0 - src_color.a);

      final_color = float4(blended_rgb, blended_a);
    }
  }

  // 計算した最終的な色を出力テクスチャに書き込む
  res.outputTex[output_coord] = final_color;
}
