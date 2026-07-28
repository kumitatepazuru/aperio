enable wgpu_binding_array;

#import aperio::color::{bt601_luma}

struct ExtractParams {
    // T = しきい値(0.0〜2.0に正規化。1.0 = 満輝度)
    threshold: f32,
    // 1 = 光色「指定なし」(元画素自身の色で光る、ソフトニー付き)
    // 0 = 光色指定(ソースの輝度だけを読み、指定色に掛ける、単純ヒンジ)
    use_source_color: i32,
    color_r: f32,
    color_g: f32,
    color_b: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: ExtractParams;

// グローの明部抽出(exedit-inspect README `inspect/glow/README.md` §3)。
//
// 「指定なし」はソフトニー(y<=T:0, T<y<2T:2*(y-T), y>=2T:y)で、十分明るい画素では
// 恒等写像に漸近しつつ、元の色相・彩度比を保つ。「指定あり」は単純なヒンジ
// (max(0, y-T))で、しきい値ぶんのオフセットがそのまま残る(README通り、しきい値を
// 上げると発光全体が暗くなる)。
//
// どちらも出力は premultiplied な「発光量」RGB(alphaは以降未使用)。「指定なし」の
// premultiplied 出力は線形性から src.rgb*src.a*(b/y) に一致し、これは
// src.rgb/luma(src.rgb) * b(色相・彩度のみを保った単位色)に等しい形へ整理できる。
fn softknee(y: f32, t: f32) -> f32 {
    if (y <= t) {
        return 0.0;
    }
    let y2 = 2.0 * t;
    if (y < y2) {
        return 2.0 * (y - t);
    }
    return y;
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let src = textureLoad(tex, coord, 0);
    let luma_straight = bt601_luma(src.rgb);
    let y = luma_straight * src.a;

    var light_rgb = vec3<f32>(0.0);
    if (params.use_source_color != 0) {
        let amount = softknee(y, params.threshold);
        if (luma_straight > 1e-6) {
            light_rgb = (src.rgb / luma_straight) * amount;
        }
    } else {
        let amount = max(0.0, y - params.threshold);
        light_rgb = vec3<f32>(params.color_r, params.color_g, params.color_b) * amount;
    }

    textureStore(outputTex, coord, vec4<f32>(light_rgb, 0.0));
}
