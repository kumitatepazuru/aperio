// NV12 (YUV420 semi-planar) → RGBA8Unorm 変換コンピュートシェーダー。
// binding(0): Y plane  (R8Unorm,  width × height)
// binding(1): UV plane (Rg8Unorm, width/2 × height/2)
// binding(2): 出力 RGBA storage texture (rgba8unorm, write)

@group(0) @binding(0) var y_tex  : texture_2d<f32>;
@group(0) @binding(1) var uv_tex : texture_2d<f32>;
@group(0) @binding(2) var out_tex: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(y_tex);
    if gid.x >= dims.x || gid.y >= dims.y {
        return;
    }

    let y  = textureLoad(y_tex,  vec2<i32>(i32(gid.x),       i32(gid.y)),       0).r;
    let uv = textureLoad(uv_tex, vec2<i32>(i32(gid.x / 2u),  i32(gid.y / 2u)),  0).rg;

    // BT.601 limited range YCbCr → RGB
    let cb = uv.x - 0.5;
    let cr = uv.y - 0.5;

    let r = clamp(y + 1.402  * cr,                         0.0, 1.0);
    let g = clamp(y - 0.34414 * cb - 0.71414 * cr,        0.0, 1.0);
    let b = clamp(y + 1.772  * cb,                         0.0, 1.0);

    textureStore(out_tex, vec2<i32>(i32(gid.x), i32(gid.y)), vec4<f32>(r, g, b, 1.0));
}
