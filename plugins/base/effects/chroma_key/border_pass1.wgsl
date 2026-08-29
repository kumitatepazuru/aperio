enable wgpu_binding_array;

#import aperio::color::{bt601_encode}
#import aperio::chroma_key::{key_metrics}

struct ChromaKeyBorderPass1Params {
    key_cb: f32,
    key_cr: f32,
    key_sat: f32,
    hue_range_turns: f32,
    sat_range: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: ChromaKeyBorderPass1Params;

// `境界補正 > 0` のパス1(exedit-inspect chroma_key README §7)。
// map_b(= clamp(d, 4096) を 1.0 基準に正規化したもの)と map_c(= hue_excess、
// 色彩補正専用)を .rg に詰めて出力する。`色彩補正` の有無に関わらず両方を常に
// 計算する(値を使うかどうかは後段パス3の分岐が決めるので、ここでは分岐不要)。
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

    let metrics = key_metrics(cb, cr, params.key_cb, params.key_cr, params.key_sat, params.hue_range_turns, params.sat_range);
    let map_b = clamp(metrics.d_norm, 0.0, 1.0);
    let map_c = metrics.hue_excess;

    textureStore(outputTex, coord, vec4<f32>(map_b, map_c, 0.0, 0.0));
}
