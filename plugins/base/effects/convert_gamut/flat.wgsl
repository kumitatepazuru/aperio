enable wgpu_binding_array;

#import aperio::color::{bt601_encode, bt601_decode}
#import aperio::convert_gamut::{gamut_distance, gamut_apply}

struct FlatParams {
    key_hue: f32,
    key_sat: f32,
    key_y: f32,
    after_cr: f32,
    after_cb: f32,
    after_y: f32,
    hue_range: f32,
    sat_range: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: FlatParams;

// `境界補正 = 0` の経路。単一パスで完結する。
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

    let d = gamut_distance(cb, cr, params.key_hue, params.key_sat, params.hue_range, params.sat_range);
    let out_ycc = gamut_apply(cr, cb, y, params.key_sat, params.key_y, params.after_cr, params.after_cb, params.after_y, d);
    let rgb = bt601_decode(out_ycc.x, out_ycc.y, out_ycc.z);

    textureStore(outputTex, coord, vec4<f32>(rgb, src.a));
}
