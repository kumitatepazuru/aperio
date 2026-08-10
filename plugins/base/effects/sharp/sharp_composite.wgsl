enable wgpu_binding_array;

#import aperio::color::{bt601_encode, bt601_decode}

struct SharpCompositeParams {
    // strength_ui/100.0(UI 100.0 = ×1.0、README §3)。
    strength: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: SharpCompositeParams;

// アンシャープマスクの合成本体(exedit-inspect sharp README §5)。inputTex[0]は
// 元画像(ストレートRGB)、inputTex[1]は4パスのボックスぼかし後(ストレートRGB、
// common/box_blur_dir.wgsl)。ボックスぼかしはアルファ加重平均でBT.601変換と
// 可換なので、ぼかし自体はRGBのまま行い、YCbCrへはこの合成の直前でのみ変換する
// (実機は先にYCbCrへ変換してからぼかすが結果は同じ)。
//
// p==orig.a*blur.a==1(両方不透明)の高速路は一般式に代数的に一致するため分岐は
// 無い(README §5)。p<1/4096(ほぼ完全透明)のときだけぼかしを捨てて元画像を
// そのまま返す(README §5.3)。アルファは常に元画像のもの。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let orig_tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(orig_tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let orig = textureLoad(orig_tex, coord, 0);
    let blur = textureLoad(inputTex[1], coord, 0);

    let orig_ycc = bt601_encode(orig.rgb);
    let blur_ycc = bt601_encode(blur.rgb);

    let p = orig.a * blur.a;
    var ycc = orig_ycc;
    if (p >= 1.0 / 4096.0) {
        let diff = (orig_ycc * orig.a - blur_ycc * blur.a) / p;
        ycc = orig_ycc + params.strength * diff;
    }

    var cr = ycc.x;
    var cb = ycc.y;
    var y = ycc.z;

    // README §6: 負の輝度はクランプせず彩度を落として黒へ着地させる。
    if (y <= -0.25) {
        y = 0.0;
        cb = 0.0;
        cr = 0.0;
    } else if (y < 0.0) {
        let t = (y + 0.25) / 0.25;
        cr *= t;
        cb *= t;
        y = 0.0;
    }

    let rgb = bt601_decode(cr, cb, y);
    textureStore(outputTex, coord, vec4<f32>(rgb, orig.a));
}
