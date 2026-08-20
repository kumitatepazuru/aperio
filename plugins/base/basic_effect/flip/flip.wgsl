enable wgpu_binding_array;

#import aperio::color::bt601_encode
#import aperio::color::bt601_decode

struct FlipParams {
    flip_v: i32,
    flip_h: i32,
    invert_luma: i32,
    invert_chroma: i32,
    invert_alpha: i32,
    _pad1: i32,
    _pad2: i32,
    _pad3: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;
@group(1) @binding(0) var<storage, read> params: FlipParams;

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let out_coord = vec2<i32>(global_id.xy);
    let tex = inputTex[0];
    let in_dims = vec2<i32>(textureDimensions(tex));

    if (out_coord.x >= in_dims.x || out_coord.y >= in_dims.y) {
        return;
    }

    var in_x = out_coord.x;
    var in_y = out_coord.y;

    if (params.flip_h != 0) {
        in_x = in_dims.x - 1 - in_x;
    }
    if (params.flip_v != 0) {
        in_y = in_dims.y - 1 - in_y;
    }

    var color = textureLoad(tex, vec2<i32>(in_x, in_y), 0);

    // 輝度・色相反転
    if (params.invert_luma != 0 || params.invert_chroma != 0) {
        var ycbcr = bt601_encode(color.rgb);
        // ycbcr: vec3(cr, cb, y)
        if (params.invert_luma != 0) {
            ycbcr.z = 1.0 - ycbcr.z;
        }
        if (params.invert_chroma != 0) {
            ycbcr.x = -ycbcr.x;
            ycbcr.y = -ycbcr.y;
        }
        color = vec4<f32>(bt601_decode(ycbcr.x, ycbcr.y, ycbcr.z), color.a);
    }

    // 透明度反転
    if (params.invert_alpha != 0) {
        color.a = 1.0 - color.a;
    }

    textureStore(outputTex, out_coord, color);
}
