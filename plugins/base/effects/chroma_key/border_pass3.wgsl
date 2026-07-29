enable wgpu_binding_array;

#import aperio::color::{bt601_encode, bt601_decode}
#import aperio::blur::{plain_box_sum, border_stretch}
#import aperio::chroma_key::{unmix_factor, unmix}

struct ChromaKeyBorderPass3Params {
    radius: i32,
    out_width: i32,
    out_height: i32,
    // 伸張定数(exedit-inspect chroma_key README §7「パス3-積とコントラスト伸張」)。
    // A = 1 - 1/r, B = 1 - (1/r)*r(整数除算込み)。r=1〜5の少数なのでPython側で
    // 事前に計算し、4096基準を1.0基準へ正規化した値をそのまま渡す。
    a_const: f32,
    b_const: f32,
    key_cb: f32,
    key_cr: f32,
    key_sat: f32,
    color_correction: i32,
    alpha_correction: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: ChromaKeyBorderPass3Params;

// `境界補正 > 0` のパス3(exedit-inspect chroma_key README §7)。
// inputTex[0]: パス2で垂直方向にボックス平均済みの map_b/map_c(このパスで水平方向にも
//              平均し、二重ボックス平均された map_a とする)
// inputTex[1]: パス1そのまま(未ぼかし)の map_b/map_c。map_bはそのピクセルの生値との
//              積に、map_cは`色彩補正`時の色差補正係数に使う
// inputTex[2]: 元画像(color + alpha)
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let blurred_tex = inputTex[0];
    let raw_map_tex = inputTex[1];
    let orig_tex = inputTex[2];
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= params.out_width || coord.y >= params.out_height) {
        return;
    }

    let radius = params.radius;
    let sum = plain_box_sum(blurred_tex, coord, vec2<i32>(1, 0), radius);
    let avg_b = sum.x / f32(2 * radius + 1);

    let raw_map = textureLoad(raw_map_tex, coord, 0);
    let map_b = raw_map.x;
    let map_c = raw_map.y;

    // README: 「ブレンドではなく積」。二重ボックス平均した値と、このピクセル自身の
    // 生の map_b との積を取ることで、キー側(map_b=0)は平均後もにじまず0のまま保たれる。
    let r_f = f32(radius);
    let bs = border_stretch(avg_b, map_b, r_f, params.a_const, params.b_const);

    let src = textureLoad(orig_tex, coord, 0);
    var alpha = src.a * bs.factor;

    let ycc = bt601_encode(src.rgb);
    let cr = ycc.x;
    let cb = ycc.y;
    let y = ycc.z;
    let sat = max(abs(cb), abs(cr));

    var base_term = bs.t;
    if (params.color_correction != 0) {
        base_term = map_c;
    }
    let f = unmix_factor(base_term, sat, params.key_sat);
    let unmixed = unmix(cb, cr, params.key_cb, params.key_cr, f);

    if (params.color_correction != 0 && params.alpha_correction != 0 && f > 1e-4 && f < 1.0) {
        alpha = alpha * f;
    }

    let rgb = bt601_decode(unmixed.y, unmixed.x, y);
    textureStore(outputTex, coord, vec4<f32>(rgb, alpha));
}
