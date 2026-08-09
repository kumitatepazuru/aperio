enable wgpu_binding_array;

#import aperio::color::{bt601_luma}

struct CompositeParams {
    // 0以外: `光成分のみ`。最終合成を丸ごとスキップし、蓄積バッファを不透明
    // キャンバスとしてそのまま出力する(exedit-inspect glow README §8)。
    light_only: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: CompositeParams;

// inputTex[0]: 元画像を common/expand.wgsl で配置したもの(straight rgba)
// inputTex[1]: 最終蓄積(.rgb=発光量。premultiplied相当、輝度がそのままアルファに
//              なる。.aは仕上げ`ぼかし`パスで潰れているため使わず、常にここで
//              bt601_lumaから再計算する)
//
// README §8 の最終合成(オブジェクト版)は「元画像が透明/不透明/半透明」で3分岐
// するが、na=min(g_luma+base.a, 1.0) を軸にした1本の式へ単純化できる:
// base.a=0 では na=min(g_luma,1)、premul/na が (a)の「輝度をアルファへ移す
// アンプリマルチプライ」と一致し、base.a=1 では na=1 となって
// premul/na = base.rgb + accum.rgb と (b)の単純加算に一致する(境界で連続)。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let base_tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(base_tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let accum = textureLoad(inputTex[1], coord, 0).rgb;
    let g_luma = bt601_luma(accum);

    if (params.light_only != 0) {
        textureStore(outputTex, coord, vec4<f32>(accum, 1.0));
        return;
    }

    let base = textureLoad(base_tex, coord, 0);

    let na = clamp(g_luma + base.a, 0.0, 1.0);
    let premul = base.rgb * base.a + accum;

    var out_rgb = vec3<f32>(0.0);
    if (na > 1e-6) {
        out_rgb = premul / na;
    }

    textureStore(outputTex, coord, vec4<f32>(out_rgb, na));
}
