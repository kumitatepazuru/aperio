enable wgpu_binding_array;

struct ShadowApplyParams {
    // 0 = 逆光オフ(素のアルファ平均、+4096バイアス相当)
    // 1 = 逆光オン(反転アルファ平均、バイアス無し)
    backlight: i32,
    // 陰影係数 B / 4096.0
    b: f32,
    color_r: f32,
    color_g: f32,
    color_b: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: ShadowApplyParams;

// 陰影パス本体(exedit-inspect light README §3)。inputTex[0]は元オブジェクト
// (base rgb+alpha, straight)、inputTex[1]はボックス平均済みバッファ(.aだけ使う。
// 逆光オフなら生アルファの平均、オンならinvert_alpha.wgsl経由の反転アルファの平均)。
//
// 実機はY/Cb/Crの3チャンネルへ加算するが、BT.601変換は線形写像(オフセット無し)
// なので decode(Y+dY, Cb+dCb, Cr+dCr) = decode(Y,Cb,Cr) + light_rgb*m と等価になる
// (light_rgb = 光色そのもの)。したがってYCbCr変換を経由せず、光色RGBを直接
// baseへ加算するだけでよい(glint.wgslが同種の理由でRGB空間のまま計算しているのと
// 同じ理屈)。アルファには一切触れない。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let base_tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(base_tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let base = textureLoad(base_tex, coord, 0);
    let avg = textureLoad(inputTex[1], coord, 0).a;

    var m: f32;
    if (params.backlight != 0) {
        m = avg * params.b;
    } else {
        let bs = params.b * 2.0 / 11.0;
        m = (avg + 1.0) * bs;
    }
    m = clamp(m, 0.0, 1.0);

    let light_color = vec3<f32>(params.color_r, params.color_g, params.color_b);
    textureStore(outputTex, coord, vec4<f32>(base.rgb + light_color * m, base.a));
}
