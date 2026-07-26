struct UpsampleParams {
    factor: i32,
    out_width: i32,
    out_height: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: UpsampleParams;

// 「高速化」ONの近似ぼかし用: downsample.wgsl で縮小しボックスぼかしをかけた
// 小画像を、元の解像度へバイリニア補間で戻す。downsample は小画像の画素 j を
// 元座標 j*factor + (factor-1)/2 の位置に対応づけるので、その逆写像
//   src = (out_coord - (factor-1)/2) / factor
// を小画像座標として4点バイリニア補間する(sampler不要、textureLoad 4点)。
// 端は最近傍にクランプ(np.interp が両端でクランプするのと同じ)。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let out_coord = vec2<i32>(global_id.xy);
    if (out_coord.x >= params.out_width || out_coord.y >= params.out_height) {
        return;
    }

    let tex = inputTex[0];
    let small_dims = vec2<i32>(textureDimensions(tex));
    let f = f32(params.factor);

    let src = (vec2<f32>(out_coord) - vec2<f32>((f - 1.0) * 0.5)) / f;
    let s0 = floor(src);
    let frac = src - s0;
    let i0 = vec2<i32>(s0);
    let i1 = i0 + vec2<i32>(1, 1);

    let lo = vec2<i32>(0, 0);
    let hi = small_dims - vec2<i32>(1, 1);
    let c00 = textureLoad(tex, clamp(vec2<i32>(i0.x, i0.y), lo, hi), 0);
    let c10 = textureLoad(tex, clamp(vec2<i32>(i1.x, i0.y), lo, hi), 0);
    let c01 = textureLoad(tex, clamp(vec2<i32>(i0.x, i1.y), lo, hi), 0);
    let c11 = textureLoad(tex, clamp(vec2<i32>(i1.x, i1.y), lo, hi), 0);

    let top = mix(c00, c10, frac.x);
    let bottom = mix(c01, c11, frac.x);
    let result = mix(top, bottom, frac.y);

    textureStore(outputTex, out_coord, result);
}
