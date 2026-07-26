struct ThresholdParams {
    gain: f32,
    overflow: f32,
    threshold: f32,
    // 0: 光色指定モード(cr_dev/cb_dev/luma_colorの定数係数を使う)
    // それ以外: 光色「指定なし」モード(元画素自身の色差を使い、輝度係数=1.0)
    use_source_chroma: i32,
    cr_dev: f32,
    cb_dev: f32,
    luma_color: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: ThresholdParams;

// 明部抽出(実機 sub_100535a0/sub_100536d0、README手順3)。
//
// 輝度がしきい値を超えた量にgain(強さ100%=1.0)を掛け、100%を超えた分は別枠の
// overflowとして加算する(強さ>100%で、しきい値ぎりぎりの暗めのピクセルまで
// 底上げされて光り出すAviUtl版の挙動を再現)。
//
// 重要: 輝度は**アルファ加重**してからしきい値と比較する(実機の非対称。色差側は
// アルファ加重しない)。aperioはストレートアルファで、text_render.wgslの fs_mask が
// カバレッジ0の画素にもテキスト色をそのまま残すため、これをしないとグリフ矩形の
// 内側全域が輝度1.0として抽出され「白いもや」になる。
//
// 出力は後段のぼかしに乗せる係数付きの抽出量:
//   .r = amount * Cr係数   (色差・後で combined_init が offset を足す)
//   .g = amount * Cb係数
//   .b = amount * 輝度係数 (色指定なら光色のYを既にここで掛ける = 中核(2))
//   .a = amount            (デバッグ用。combined_init は r/g/b しか読まない)
// Cr/Cb/輝度の各係数は光色モードで切り替わる。reconstruct.wgsl の BT.601 逆変換が
// どちらのモードでも最終色を復元する。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let color = textureLoad(tex, coord, 0);
    let y_src = dot(color.rgb, vec3<f32>(0.299, 0.587, 0.114));
    let luminance = y_src * color.a;

    var amount = 0.0;
    if (luminance > params.threshold) {
        amount = clamp((luminance - params.threshold) * params.gain + params.overflow, 0.0, 1.0);
    }

    var cr_coeff = params.cr_dev;
    var cb_coeff = params.cb_dev;
    var luma_coeff = params.luma_color;
    if (params.use_source_chroma != 0) {
        // 指定なし: 元画素自身の色差(アルファ非加重の素の色から算出)、輝度係数=1.0
        cr_coeff = (color.r - y_src) / 1.402000;
        cb_coeff = (color.b - y_src) / 1.772000;
        luma_coeff = 1.0;
    }

    textureStore(
        outputTex,
        coord,
        vec4<f32>(amount * cr_coeff, amount * cb_coeff, amount * luma_coeff, amount),
    );
}
