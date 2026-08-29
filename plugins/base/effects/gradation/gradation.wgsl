enable wgpu_binding_array;

#import aperio::color::{bt601_encode, bt601_decode}

struct GradationParams {
    cx: f32,
    cy: f32,
    // angle_vector規約(sin=x, -cos=y)で事前計算済みの単位方向ベクトル。
    vx: f32,
    vy: f32,
    width: f32,
    // 0=線, 1=円, 2=四角形, 3=凸形。
    shape: i32,
    start_r: f32,
    start_g: f32,
    start_b: f32,
    start_a: f32,
    end_r: f32,
    end_g: f32,
    end_b: f32,
    end_a: f32,
    // 0=通常,1=加算,2=減算,3=乗算,4=スクリーン,5=オーバーレイ,6=比較(明),7=比較(暗),
    // 8=輝度,9=色差,10=陰影,11=明暗,12=差分。
    blend_mode: i32,
    out_width: i32,
    out_height: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: GradationParams;

const PI: f32 = 3.14159265358979;

// exedit-inspect gradation README/blend_modes.md: 13種の合成モードのうち
// ビット単位の式が非公開なもの(陰影/明暗など)は、合成モード
// 解説(Photoshopの定型文と一致することを確認済み)に基づく式で近似する。
// 輝度/色差はRGB経由ではなくYCbCrネイティブなので blend_rgb 側で個別に扱う。
fn blend_channel(mode: i32, d: f32, s: f32) -> f32 {
    switch (mode) {
        case 1: { return d + s; }
        case 2: { return d - s; }
        case 3: { return d * s; }
        case 4: { return 1.0 - (1.0 - d) * (1.0 - s); }
        case 5: {
            if (d <= 0.5) { return 2.0 * d * s; }
            return 1.0 - 2.0 * (1.0 - d) * (1.0 - s);
        }
        case 6: { return max(d, s); }
        case 7: { return min(d, s); }
        // 陰影(Photoshopの「焼き込み(リニア)/Linear Burn」と同じ式。
        // Photoshopの定型文そのままであることから確認)。
        case 10: { return clamp(d + s - 1.0, 0.0, 1.0); }
        // 明暗(Photoshopの「リニアライト/Linear Light」と同じ式。陰影と同様に
        // Photoshopの定型文が一致することから確認)。
        case 11: { return clamp(d + 2.0 * s - 1.0, 0.0, 1.0); }
        case 12: { return abs(d - s); }
        default: { return s; }
    }
}

fn blend_rgb(mode: i32, dst: vec3<f32>, src: vec3<f32>) -> vec3<f32> {
    // 輝度(8)・色差(9)は exedit-inspect blend_modes.md §3 の通りRGB経由の
    // モード群([0x03..0x07], [0x0a..0x0c])に含まれず、PIXEL_YCAのY/Cb/Crを直接触るネイティブなモード
    if (mode == 8) {
        // 輝度: dstのCb/Crを保ち、Yだけsrcのもので置き換える
        // (色相・彩度はdstのまま、明暗構造だけをsrcに合わせる)。
        let ycc_d = bt601_encode(dst);
        let ycc_s = bt601_encode(src);
        return bt601_decode(ycc_d.x, ycc_d.y, ycc_s.z);
    }
    if (mode == 9) {
        // 色差: dstのYを保ち、Cb/Crをsrcのもので置き換える
        // (dst自身の明暗構造は保ち、色相・彩度だけをsrcに合わせる)。
        let ycc_d = bt601_encode(dst);
        let ycc_s = bt601_encode(src);
        return bt601_decode(ycc_s.x, ycc_s.y, ycc_d.z);
    }
    return vec3<f32>(
        blend_channel(mode, dst.x, src.x),
        blend_channel(mode, dst.y, src.y),
        blend_channel(mode, dst.z, src.z),
    );
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let coord = vec2<i32>(global_id.xy);
    if (coord.x >= params.out_width || coord.y >= params.out_height) {
        return;
    }

    let dst = textureLoad(inputTex[0], coord, 0);

    let dx = f32(coord.x) + 0.5 - params.cx;
    let dy = f32(coord.y) + 0.5 - params.cy;
    let s_par = params.vx * dx + params.vy * dy;
    let s_perp = params.vy * dx - params.vx * dy;

    var u: f32;
    if (params.shape == 1) {
        u = params.width - sqrt(dx * dx + dy * dy);
    } else if (params.shape == 2) {
        u = params.width - abs(s_par) - abs(s_perp);
    } else if (params.shape == 3) {
        u = params.width - abs(s_par);
    } else {
        u = params.width * 0.5 + s_par;
    }

    let end_color = vec4<f32>(params.end_r, params.end_g, params.end_b, params.end_a);
    let start_color = vec4<f32>(params.start_r, params.start_g, params.start_b, params.start_a);

    var grad: vec4<f32>;
    if (u <= 0.0) {
        grad = end_color;
    } else if (u >= params.width) {
        grad = start_color;
    } else {
        let tt = u / params.width;
        let e = 0.5 - 0.5 * cos(PI * tt);
        grad = mix(end_color, start_color, e);
    }

    let blended_rgb = blend_rgb(params.blend_mode, dst.rgb, grad.rgb);
    let src_a = clamp(grad.a, 0.0, 1.0);

    // exedit-inspect blend_modes.md §3 表B: dstのアルファには一切触れない
    // 単純な線形補間(out_a = a_d 固定)。表A(Porter-Duffのsrc-over)とは
    // dst.a < 1 のとき結果が異なる。
    let out_rgb = dst.rgb + src_a * (blended_rgb - dst.rgb);

    textureStore(outputTex, coord, vec4<f32>(out_rgb, dst.a));
}
