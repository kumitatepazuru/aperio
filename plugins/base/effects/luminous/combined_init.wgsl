enable wgpu_binding_array;

struct CombinedInitParams {
    base: f32,
    offset: f32,
    // 0: 拡散速度0(輝度は線形のまま)。それ以外: 拡散速度>0(輝度に指数カーブを掛ける)
    apply_curve: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: CombinedInitParams;

// threshold.wgsl が作った抽出量(r=amount*Cr係数, g=amount*Cb係数, b=amount*輝度係数)
// を、6パスぼかしに乗せられる形へ整える初期化シェーダー。輝度・色差を1本の
// チェーンにまとめて回すための共通の入口(拡散速度0/>0の両方が通る)。
//
//   .r = in.r + offset   (色差。box_blur_h/vが負値をmax(0,x)で潰すため十分大きい
//   .g = in.g + offset    offsetを足しておき、蓄積時に差し引く。輝度は非負なので不要)
//   .b = apply_curve ? pow(base, in.b*256)-1 : in.b
//        (拡散速度>0のときだけ指数カーブ。in.b は既に amount*輝度係数 なので
//         光色のYを先に掛けた値がそのまま指数の肩に乗る = 中核(2))
//   .a = 1.0 (box_blur_h/vのプリマルチプライドをno-opにして、r/g/bをそれぞれ
//        独立な単純重み平均としてぼかすためのダミー値)
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let src = textureLoad(tex, coord, 0);
    let cr = src.r + params.offset;
    let cb = src.g + params.offset;
    var luma = src.b;
    if (params.apply_curve != 0) {
        luma = pow(params.base, src.b * 256.0) - 1.0;
    }

    textureStore(outputTex, coord, vec4<f32>(cr, cb, luma, 1.0));
}
