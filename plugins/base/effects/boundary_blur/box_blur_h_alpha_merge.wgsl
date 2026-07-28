enable wgpu_binding_array;

#import aperio::blur::{box_sum}

struct BoxBlurAlphaMergeParams {
    radius: i32,
    out_width: i32,
    out_height: i32,
    offset: i32,
    border_mode: i32,
    divisor_mode: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: BoxBlurAlphaMergeParams;

// common/lib/blur.wgsl の box_sum (common/box_blur_dir.wgsl と共用) の水平パスと
// alpha_merge.wgsl の非線形しきい値合成を1シェーダーに統合したもの(境界ぼかしの
// 透明度境界モード専用)。alpha_merge.wgsl が読むのはぼかし後の.aだけなので、
// box_sumが返すsum_rgbはここでは使わない。
//
// inputTex[0]: 垂直パス済みのアルファ(box_blur_dir通過後。ry<=0なら元画像そのもの)
// inputTex[1]: 元画像(色 + 元のアルファA)
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let orig_tex = inputTex[1];
    let out_coord = vec2<i32>(global_id.xy);
    let radius = params.radius;

    if (out_coord.x >= params.out_width || out_coord.y >= params.out_height) {
        return;
    }

    let in_coord = out_coord - vec2<i32>(params.offset, 0);

    let r = box_sum(tex, in_coord, vec2<i32>(1, 0), radius, params.border_mode);

    let divisor = select(f32(2 * radius + 1), f32(max(r.valid_count, 1)), params.divisor_mode != 0);
    let b = r.sum_weight / divisor;

    // README 4: 単純なボックスぼかし結果をそのまま使うのではなく、非線形なしきい値
    // カーブで再合成する(実機の12bit整数演算 t=((A*B)>>11)-4096;
    // alpha'=(A*t)>>12 if t>0 else 0 を [0,1] 正規化した連続式)。
    let orig = textureLoad(orig_tex, out_coord, 0);
    let a = orig.a;
    let t = 2.0 * a * b - 1.0;
    let alpha = max(a * t, 0.0);

    textureStore(outputTex, out_coord, vec4<f32>(orig.rgb, alpha));
}
