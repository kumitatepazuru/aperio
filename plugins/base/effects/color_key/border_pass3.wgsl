enable wgpu_binding_array;

#import aperio::blur::{plain_box_sum, border_stretch}

struct ColorKeyBorderPass3Params {
    radius: i32,
    out_width: i32,
    out_height: i32,
    // 伸張定数(exedit-inspect color_key README §5「境界補正 ― アルファそのものをぼかす」)。
    // a = 1 - 1/r。v=1(完全不透明)のとき t(v)=1になるコントラスト伸張カーブの係数。
    a_const: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: ColorKeyBorderPass3Params;

// `境界補正 > 0` のパス3(exedit-inspect color_key README §5)。
// inputTex[0]: パス2で垂直方向にボックス平均済みのa0(このパスで水平方向にも平均する)
// inputTex[1]: パス1そのまま(未ぼかし)のa0
// inputTex[2]: 元画像(color + alpha)。rgbはここからそのまま素通しする
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let blurred_tex = inputTex[0];
    let raw_a0_tex = inputTex[1];
    let orig_tex = inputTex[2];
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= params.out_width || coord.y >= params.out_height) {
        return;
    }

    let radius = params.radius;
    let sum = plain_box_sum(blurred_tex, coord, vec2<i32>(1, 0), radius);
    let avg = sum.x / f32(2 * radius + 1);

    let a0 = textureLoad(raw_a0_tex, coord, 0).x;

    // README §5/§6: 「ぼかす対象も掛ける相手も画素のアルファ本体」。マットではなくa0自身を
    // 二重ボックス平均した値とa0の積を取るため、半透明な一様領域でも v = a0²/4096 に潰れうる。
    let r_f = f32(radius);
    let bs = border_stretch(avg, a0, r_f, params.a_const);
    let alpha = a0 * bs.factor;

    let src = textureLoad(orig_tex, coord, 0);
    textureStore(outputTex, coord, vec4<f32>(src.rgb, alpha));
}
