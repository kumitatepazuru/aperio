enable wgpu_binding_array;

#import aperio::color::{bt601_encode}
#import aperio::color_key::{key_alpha}

struct ColorKeyBorderPass1Params {
    key_y: f32,
    key_cb: f32,
    key_cr: f32,
    luma_range: f32,
    chroma_range: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: ColorKeyBorderPass1Params;

// `境界補正 > 0` のパス1(exedit-inspect color_key README §5)。キー判定後の
// アルファ(a0 = 直方体内なら0、それ以外は元のアルファ)を .r だけに詰めて出力する。
// chroma_keyのpass1と違い「色彩補正」用の第2マップは無いので1チャンネルのみ。
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

    let a0 = key_alpha(y, cb, cr, params.key_y, params.key_cb, params.key_cr, params.luma_range, params.chroma_range, src.a);

    textureStore(outputTex, coord, vec4<f32>(a0, 0.0, 0.0, 0.0));
}
