enable wgpu_binding_array;

#import aperio::color::{bt601_encode, bt601_decode}

struct MonochromaticParams {
    strength: f32,
    // 1なら輝度(Y)は維持してCb/Crのみターゲット色へ寄せる。0ならYも寄せる。
    preserve_luminance: i32,
    color_r: f32,
    color_g: f32,
    color_b: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: MonochromaticParams;

// exedit-inspect noise/monochromatic README: 全画素のCb/Crを strength でターゲット色へ
// 線形補間する。preserve_luminance が真ならYは触らない(明暗はそのまま、色味だけが
// ターゲット色に染まる)。alphaは一切読み書きしない。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let src = textureLoad(tex, coord, 0);
    let src_ycc = bt601_encode(src.rgb);
    let key_ycc = bt601_encode(vec3<f32>(params.color_r, params.color_g, params.color_b));

    let t = clamp(params.strength, 0.0, 1.0);
    let cr = mix(src_ycc.x, key_ycc.x, t);
    let cb = mix(src_ycc.y, key_ycc.y, t);
    var y = src_ycc.z;
    if (params.preserve_luminance == 0) {
        y = mix(src_ycc.z, key_ycc.z, t);
    }

    let rgb = bt601_decode(cr, cb, y);
    textureStore(outputTex, coord, vec4<f32>(rgb, src.a));
}
