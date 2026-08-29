enable wgpu_binding_array;

#import aperio::color::{bt601_encode, bt601_decode}
#import aperio::chroma_key::{key_metrics, unmix_factor, unmix}

struct ChromaKeyFlatParams {
    key_cb: f32,
    key_cr: f32,
    key_sat: f32,
    hue_range_turns: f32,
    sat_range: f32,
    color_correction: i32,
    alpha_correction: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: ChromaKeyFlatParams;

// `境界補正 = 0` の経路(exedit-inspect chroma_key README §5/§6)。イン・プレースで
// 1回のワーカーが走るだけの実機と同じく、単一パスで完結する。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let src = textureLoad(tex, coord, 0);
    let ycc = bt601_encode(src.rgb);
    let cr = ycc.x;
    let cb = ycc.y;
    let y = ycc.z;

    let metrics = key_metrics(cb, cr, params.key_cb, params.key_cr, params.key_sat, params.hue_range_turns, params.sat_range);

    var alpha = src.a * clamp(metrics.d_norm, 0.0, 1.0);
    var out_cb = cb;
    var out_cr = cr;

    if (params.color_correction != 0) {
        let f = unmix_factor(metrics.hue_excess, metrics.sat, params.key_sat);
        let unmixed = unmix(cb, cr, params.key_cb, params.key_cr, f);
        out_cb = unmixed.x;
        out_cr = unmixed.y;
        if (params.alpha_correction != 0 && f > 1e-4 && f < 1.0) {
            alpha = alpha * f;
        }
    }

    let rgb = bt601_decode(out_cr, out_cb, y);
    textureStore(outputTex, coord, vec4<f32>(rgb, alpha));
}
