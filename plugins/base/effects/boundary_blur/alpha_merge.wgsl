@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

// inputTex[0]: 元の画像(色 + 元のアルファA)
// inputTex[1]: common/box_blur_h・v で2パスのボックスぼかしを通したアルファB
//              (.rgbは色の重み付き平均になっているが未使用、.aのみ使う)
//
// README 4: 単純なボックスぼかし結果をそのまま使うのではなく、非線形なしきい値
// カーブで再合成する(実機の12bit整数演算 t=((A*B)>>11)-4096;
// alpha'=(A*t)>>12 if t>0 else 0 を [0,1] 正規化した連続式)。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let orig_tex = inputTex[0];
    let blurred_tex = inputTex[1];
    let dims = vec2<i32>(textureDimensions(orig_tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let orig = textureLoad(orig_tex, coord, 0);
    let a = orig.a;
    let b = textureLoad(blurred_tex, coord, 0).a;

    let t = 2.0 * a * b - 1.0;
    let alpha = max(a * t, 0.0);

    textureStore(outputTex, coord, vec4<f32>(orig.rgb, alpha));
}
