// 各レイヤーのメタ情報を格納する構造体
struct LayerParams {
  x: i32,     // レイヤーの左上のx座標
  y: i32,     // レイヤーの左上のy座標
  scale: f32,  // レイヤーの拡大・縮小率
  alpha: f32,  // レイヤーの透明度 (0.0〜1.0)
  rotation_matrix: mat2x2<f32>, // レイヤーの回転行列
};

// --- リソースのバインディング定義 ---

// グループ0: テクスチャ関連
@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;
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

  // このピクセルの最終的な色。初期値は透明な黒 (背景)
  var final_color = vec4<f32>(0.0, 0.0, 0.0, 0.0);

  let num_layers = arrayLength(&layer_params_array);
  
  // 全てのレイヤーを順番に重ね合わせる
  for (var i: u32 = 0u; i < num_layers; i = i + 1u) {
    let params = layer_params_array[i];
    let layer_dims = textureDimensions(inputTex[i]);
    let layer_dims_f = vec2<f32>(layer_dims);
    if (params.scale <= 0.0) {
      continue;
    }

    // 出力ピクセル座標から、レイヤーテクスチャ上の対応する座標を計算
    let output_center = vec2<f32>(f32(params.x), f32(params.y));
    let relative_coord = vec2<f32>(output_coord) - output_center;
    
    // 事前に計算された回転行列を適用
    let rotated_coord = params.rotation_matrix * relative_coord;

    
    // スケールを適用
    let src_coord_pixel = rotated_coord / params.scale;

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
