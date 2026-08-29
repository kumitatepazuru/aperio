enable wgpu_binding_array;

// TODO: fragment shaderなどをstepで扱えるようにして、簡略化・高速化をする


// 各レイヤーのメタ情報を格納する構造体
struct LayerParams {
  // 出力中心相対の画面座標 -> テクスチャ相対座標の逆ホモグラフィ。
  // 3D回転(X/Y軸を含む)・拡大率・平行移動(X/Y/Z)・透視投影がすべてこの1枚に畳み込まれている。
  // 同次座標なので、掛けたあとに z で割る必要がある。
  inv_transform: mat3x3<f32>,
  center_x: f32,                // 回転・拡縮の基点オフセットX (レイヤーテクスチャ中心からのピクセル数)
  center_y: f32,                // 回転・拡縮の基点オフセットY (レイヤーテクスチャ中心からのピクセル数)
  alpha: f32,                   // レイヤーの透明度 (0.0〜1.0)
  _pad: f32,                    // アライメント用パディング
};

// --- リソースのバインディング定義 ---

// グループ0: テクスチャ関連
@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;
@group(0) @binding(2) var linear_sampler: sampler;

// グループ1: メタデータ
@group(1) @binding(0) var<storage, read> layer_params_array: array<LayerParams>;


// --- コンピュートシェーダー本体 ---

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
  let output_coord = vec2<i32>(global_id.xy);
  let output_dims = textureDimensions(outputTex);

  // 処理対象が出力テクスチャの範囲外であれば、何もしない
  if (output_coord.x >= i32(output_dims.x) || output_coord.y >= i32(output_dims.y)) {
    return;
  }

  // このピクセルの最終的な色。初期値は黒 (背景)
  var final_color = vec4<f32>(0.0, 0.0, 0.0, 255.0);

  let num_layers = arrayLength(&layer_params_array);
  
  // 全てのレイヤーを順番に重ね合わせる
  for (var i: u32 = 0u; i < num_layers; i = i + 1u) {
    let params = layer_params_array[i];
    let layer_dims = textureDimensions(inputTex[i]);
    let layer_dims_f = vec2<f32>(layer_dims);
    if (params.alpha <= 0.0) {
      continue;
    }

    // 出力ピクセル座標から、レイヤーテクスチャ上の対応する座標を計算する。
    // 出力テクスチャの中央が原点。+0.5 はピクセル中心を指すため
    // (後段の src_coord_pixel / layer_dims_f がテクセル中心と一致するようになる)。
    let output_center = vec2<f32>(output_dims) * 0.5;
    let relative_coord = vec2<f32>(output_coord) + vec2<f32>(0.5, 0.5) - output_center;

    // 事前に計算された逆ホモグラフィ (3D回転 + 拡縮 + 平行移動 + 透視投影) を適用
    let q = params.inv_transform * vec3<f32>(relative_coord, 1.0);
    if (q.z <= 0.0) {
      continue;  // カメラ平面より後方 (このレイヤーには映らない画素)
    }
    let src_rel = q.xy / q.z;

    // 基点オフセットを加えてテクスチャピクセル座標に変換
    let src_coord_pixel = src_rel + layer_dims_f * 0.5 + vec2<f32>(params.center_x, params.center_y);

    if (src_coord_pixel.x >= 0.0 && src_coord_pixel.x < layer_dims_f.x &&
        src_coord_pixel.y >= 0.0 && src_coord_pixel.y < layer_dims_f.y) {

      // textureSampleを使うために座標を正規化
      let src_coord_normalized = src_coord_pixel / layer_dims_f;
      let src_color = textureSampleLevel(inputTex[i], linear_sampler, src_coord_normalized, 0.0);

      // --- アルファブレンディング (Over演算) ---
      // 現在の色 (destination color) の上に新しいレイヤーの色を重ねる
      let dst_color = final_color;
      let alpha = src_color.a * params.alpha;

      let blended_rgb = src_color.rgb * alpha + dst_color.rgb * (1.0 - alpha);
      let blended_a = src_color.a + dst_color.a * (1.0 - src_color.a);

      final_color = vec4<f32>(blended_rgb, blended_a);
    }
  }

  // 計算した最終的な色を出力テクスチャに書き込む
  textureStore(outputTex, output_coord, final_color);
}
