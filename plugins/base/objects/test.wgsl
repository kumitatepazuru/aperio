// 入力画像データの構造体
struct ImageInfo {
    width: u32,
    height: u32,
    // RGBデータがフラットに格納されている配列
    // dataフィールドは構造体の末尾にある必要があります
    data: array<f32>
};

// --- バインドグループの定義 ---

// 出力テクスチャ
@group(0) @binding(0)
var outputTex: texture_storage_2d<rgba32float, write>;

// 入力画像データ
@group(1) @binding(0) 
var<storage, read> input: ImageInfo;


// --- コンピュートシェーダー本体 ---

// ワークグループのサイズ。ハードウェアに応じて調整可能
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

    // 1. 出力座標から、対応する入力座標を計算する

    // 出力座標を0.0～1.0のUV座標に変換
    // (ピクセルの中央をサンプリングするために0.5を加算)
    let uv = (vec2<f32>(output_coord) + vec2<f32>(0.5, 0.5)) / vec2<f32>(output_dims);

    // UV座標を入力画像の解像度にスケールして、対応する入力座標（浮動小数点）を求める
    let input_coord_f = uv * vec2<f32>(f32(input.width), f32(input.height));

    // ニアレストネイバー法のため、最も近いピクセル座標を整数で取得
    let input_coord = vec2<u32>(floor(input_coord_f));

    // 計算した入力座標が画像の範囲内に収まるようにクランプ
    let clamped_input_coord = clamp(input_coord, vec2<u32>(0, 0), vec2<u32>(input.width - 1, input.height - 1));

    // 2. 入力データ配列からピクセル色を読み出す

    // 2次元のピクセル座標を1次元配列のインデックスに変換
    // RGBの3チャンネルなので、インデックスに3を乗算
    let index = (clamped_input_coord.y * input.width + clamped_input_coord.x) * 3;

    // 読み出したRGB値から、出力用の色（RGBA）を作成
    // アルファ値は1.0（不透明）に設定
    // OpenCVのデフォルトの色順がBGRであるため、順番に注意
    let color = vec4<f32>(
        input.data[index + 2],  // R
        input.data[index + 1],  // G
        input.data[index + 0],  // B
        1.0                     // A
    );

    // 3. 計算した色を出力テクスチャに書き込む
    textureStore(outputTex, output_coord, color);
}