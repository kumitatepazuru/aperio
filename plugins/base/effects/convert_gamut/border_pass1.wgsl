enable wgpu_binding_array;

#import aperio::color::{bt601_encode}
#import aperio::convert_gamut::{gamut_distance}

struct BorderPass1Params {
    key_hue: f32,
    key_sat: f32,
    hue_range: f32,
    sat_range: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: BorderPass1Params;

// `境界補正 > 0` のパス1。距離 d(gamut_distanceの結果、0=完全一致・1=無関係)を
// .r だけに詰めて出力する(chroma_keyのmap_bと同じ役割)。
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
    let d = gamut_distance(ycc.y, ycc.x, params.key_hue, params.key_sat, params.hue_range, params.sat_range);

    textureStore(outputTex, coord, vec4<f32>(d, 0.0, 0.0, 0.0));
}
