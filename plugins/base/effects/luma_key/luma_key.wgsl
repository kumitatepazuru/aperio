enable wgpu_binding_array;

#import aperio::color::{bt601_luma}

struct LumaKeyParams {
    base: f32,
    blur: f32,
    key_type: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: LumaKeyParams;

// exedit-inspect luma_key README §7: 近傍を一切見ず、その画素自身のy・aだけで
// アルファを決める単一パス(境界補正に相当する処理は実機に存在しない)。
// 読むのはyだけ、書くのはaだけでrgbは素通りする。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let src = textureLoad(tex, coord, 0);
    let y = bt601_luma(src.rgb);

    // README §4: type0/1は帯の片側全部が通過域になるランプ、type2/3は
    // 折り返し(t>blurならt=2*blur-t)により閉じた式 blur-|y-base| に一致する。
    var t: f32;
    if (params.key_type == 0) {
        t = y - params.base + params.blur;
    } else if (params.key_type == 1) {
        t = params.base + params.blur - y;
    } else {
        t = params.blur - abs(y - params.base);
    }

    // README §4: 境界は「残す側」が閉じている(t<0->0、t>=blurはそのまま)。
    // type3だけランプ枝を持たない(乗除算なしのハードバンド)。
    var alpha = src.a;
    if (t < 0.0) {
        alpha = 0.0;
    } else if (params.key_type != 3 && t < params.blur) {
        alpha = src.a * (t / params.blur);
    }

    textureStore(outputTex, coord, vec4<f32>(src.rgb, alpha));
}
