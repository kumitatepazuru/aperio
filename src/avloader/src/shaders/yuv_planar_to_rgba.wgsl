// フルプラナー YUV (I420/YV12/I422/I444 等) → Rgba8Unorm 変換。
// BT.709 limited range を使用。
//
// binding(0): Y プレーン (R8Unorm, width × height)
// binding(1): U または V プレーン (R8Unorm, width/w_div × height/h_div)
// binding(2): V または U プレーン (R8Unorm)
// binding(3): パラメータ uniform
// binding(4): 出力 RGBA storage texture

struct Params {
    chroma_w_div: u32,
    chroma_h_div: u32,
    swap_uv:      u32, // 1 → binding(1) が V, binding(2) が U（YV12）
    _pad:         u32,
}

@group(0) @binding(0) var y_tex  : texture_2d<f32>;
@group(0) @binding(1) var u_tex  : texture_2d<f32>; // swap_uv=1 のとき実際は V
@group(0) @binding(2) var v_tex  : texture_2d<f32>; // swap_uv=1 のとき実際は U
@group(0) @binding(3) var<uniform> params : Params;
@group(0) @binding(4) var out_tex: texture_storage_2d<rgba8unorm, write>;

fn ycbcr_to_rgb(y_norm: f32, cb_norm: f32, cr_norm: f32) -> vec3<f32> {
    // BT.709 limited range
    let y  = y_norm  - 16.0  / 255.0;
    let cb = cb_norm - 128.0 / 255.0;
    let cr = cr_norm - 128.0 / 255.0;
    let r = clamp(1.164 * y + 1.793 * cr,                 0.0, 1.0);
    let g = clamp(1.164 * y - 0.213 * cb - 0.533 * cr,   0.0, 1.0);
    let b = clamp(1.164 * y + 2.112 * cb,                 0.0, 1.0);
    return vec3<f32>(r, g, b);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(out_tex);
    if gid.x >= dims.x || gid.y >= dims.y { return; }

    let y_val  = textureLoad(y_tex, vec2<i32>(i32(gid.x), i32(gid.y)), 0).r;
    let cx     = gid.x / params.chroma_w_div;
    let cy     = gid.y / params.chroma_h_div;
    let p1_val = textureLoad(u_tex, vec2<i32>(i32(cx), i32(cy)), 0).r;
    let p2_val = textureLoad(v_tex, vec2<i32>(i32(cx), i32(cy)), 0).r;

    var cb: f32;
    var cr: f32;
    if params.swap_uv == 0u {
        cb = p1_val; cr = p2_val;
    } else {
        cr = p1_val; cb = p2_val;
    }

    let rgb = ycbcr_to_rgb(y_val, cb, cr);
    textureStore(out_tex, vec2<i32>(i32(gid.x), i32(gid.y)), vec4<f32>(rgb, 1.0));
}
