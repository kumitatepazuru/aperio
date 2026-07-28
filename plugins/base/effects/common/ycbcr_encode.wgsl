enable wgpu_binding_array;

#import aperio::color::{bt601_encode}

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

// RGB(ストレートアルファ)をBT.601の輝度Y・色差Cr/Cbへ変換し、
// {r=Cr偏差, g=Cb偏差, b=Y, a=alpha} に詰め替える。alphaは素通し。
// common/box_blur_h.wgsl・box_blur_v.wgsl はチャンネルを区別せず
// アルファ加重平均するだけなので、このYCbCr表現のままボックスぼかしを
// 通してもRGBのまま通すのと同じ結果になる。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let s = textureLoad(tex, coord, 0);
    let ycc = bt601_encode(s.rgb);

    textureStore(outputTex, coord, vec4<f32>(ycc.x, ycc.y, ycc.z, s.a));
}
