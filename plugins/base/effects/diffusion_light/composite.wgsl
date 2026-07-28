enable wgpu_binding_array;

struct CompositeParams {
    strength: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: CompositeParams;

// inputTex[0]: このラウンドのボックスぼかし結果(YCbCr+alpha、ストレート、
//              common/box_blur_dir.wgsl通過済み)
// inputTex[1]: このラウンド開始時点の「現在の画像」を common/expand.wgsl で
//              同じキャンバスへ再配置したもの(同じくYCbCr+alpha、ストレート)
//
// ぼかし結果の輝度がその画素自身の輝度より明るいときだけ、差分に応じて
// 色差をブレンドしたグロー色を「現在の画像」の上に強さ(strength)を
// アルファとして source-over で重ねる。暗くなる方向には働かない。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let blur_tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(blur_tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let blur = textureLoad(blur_tex, coord, 0);
    let src = textureLoad(inputTex[1], coord, 0);

    let gy = blur.b;
    let premul_y = max(0.0, src.b * src.a);
    let d = gy - premul_y;

    var ga = 0.0;
    var mix_cr = src.r;
    var mix_cb = src.g;
    var mix_y = src.b;
    if (gy > 1e-6 && d > 1e-6) {
        ga = clamp(blur.a * params.strength, 0.0, 1.0);
        let base = gy - d;
        mix_cr = ((src.r * src.a) * base + d * blur.r) / gy;
        mix_cb = ((src.g * src.a) * base + d * blur.g) / gy;
        mix_y = gy;
    }

    // グロー(色=mix、アルファ=ga)を src の上に通常の source-over で合成する。
    let ia = 1.0 - ga;
    let out_a = clamp(ga + src.a * ia, 0.0, 1.0);

    var out_cr = src.r;
    var out_cb = src.g;
    var out_y = src.b;
    if (out_a > 1e-6) {
        let w_glow = ga / out_a;
        let w_src = ia * src.a / out_a;
        out_cr = mix_cr * w_glow + src.r * w_src;
        out_cb = mix_cb * w_glow + src.g * w_src;
        out_y = mix_y * w_glow + src.b * w_src;
    }

    textureStore(outputTex, coord, vec4<f32>(out_cr, out_cb, out_y, out_a));
}
