// --- バインドグループの定義 ---

// 出力テクスチャ
@group(0) @binding(0)
var outputTex: texture_storage_2d<rgba32float, write>;

// 入力画像データ
@group(1) @binding(0) 
var<storage, read> input: array<f32>;


// --- コンピュートシェーダー本体 ---
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    // 出力テクスチャの解像度を取得
    let output_dims = textureDimensions(outputTex);

    // このシェーダーインスタンスが担当する出力ピクセルの座標
    let output_coord = vec2<u32>(global_id.xy);

    // ディスパッチサイズが出力テクスチャのサイズと完全に一致しない場合、
    // 範囲外の呼び出しを無視するためのガード
    if (output_coord.x >= output_dims.x || output_coord.y >= output_dims.y) {
        return;
    }

    // 入力データ配列からピクセル色を読み出す

    // 2次元のピクセル座標を1次元配列のインデックスに変換
    // RGBの3チャンネルなので、インデックスに3を乗算
    let index = (output_coord.y * output_dims.x + output_coord.x) * 3;

    // 読み出したRGB値から、出力用の色（RGBA）を作成
    // アルファ値は1.0（不透明）に設定
    // OpenCVのデフォルトの色順がBGRであるため、順番に注意
    let color = vec4<f32>(
        input[index + 2],  // R
        input[index + 1],  // G
        input[index + 0],  // B
        1.0                // A
    );

    // 3. 計算した色を出力テクスチャに書き込む
    textureStore(outputTex, output_coord, color);
}