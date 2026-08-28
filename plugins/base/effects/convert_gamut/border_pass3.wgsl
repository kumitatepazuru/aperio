enable wgpu_binding_array;

#import aperio::blur::{plain_box_sum, border_stretch}
#import aperio::color::{bt601_encode, bt601_decode}
#import aperio::convert_gamut::{gamut_apply}

struct BorderPass3Params {
    radius: i32,
    out_width: i32,
    out_height: i32,
    a_const: f32,
    key_sat: f32,
    key_y: f32,
    after_cr: f32,
    after_cb: f32,
    after_y: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: BorderPass3Params;

// `境界補正 > 0` のパス3。
// inputTex[0]: パス2で垂直方向に平均済みの距離d(このパスで水平方向にも平均する)
// inputTex[1]: パス1そのまま(未ぼかし)の距離d
// inputTex[2]: 元画像(color + alpha)
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let blurred_tex = inputTex[0];
    let raw_tex = inputTex[1];
    let orig_tex = inputTex[2];
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= params.out_width || coord.y >= params.out_height) {
        return;
    }

    let radius = params.radius;
    let sum = plain_box_sum(blurred_tex, coord, vec2<i32>(1, 0), radius);
    let avg = sum.x / f32(2 * radius + 1);

    let raw_d = textureLoad(raw_tex, coord, 0).x;
    let r_f = f32(radius);
    let bs = border_stretch(avg, raw_d, r_f, params.a_const);

    let src = textureLoad(orig_tex, coord, 0);
    // exedit-inspect convert_gamut README §7: アルファには書き戻さず、伸張済みの t に
    // 画素アルファを掛けたものをそのまま混合率として使う(chroma_keyはこれをalphaに書く)。
    let d_final = clamp(src.a * bs.factor, 0.0, 1.0);

    let ycc = bt601_encode(src.rgb);
    let out_ycc = gamut_apply(
        ycc.x, ycc.y, ycc.z,
        params.key_sat, params.key_y,
        params.after_cr, params.after_cb, params.after_y,
        d_final,
    );
    let rgb = bt601_decode(out_ycc.x, out_ycc.y, out_ycc.z);

    textureStore(outputTex, coord, vec4<f32>(rgb, src.a));
}
