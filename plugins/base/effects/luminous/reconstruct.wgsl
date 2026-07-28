enable wgpu_binding_array;

#import aperio::color::{bt601_decode}

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

// 輝度アキュムレータ(inputTex[1])と色差アキュムレータ(inputTex[2])を
// BT.601フルレンジの逆変換で合成し、発光色(プリマルチプライドをun-premultiply
// したrgb＋alpha)を復元したうえで、元オブジェクト(inputTex[0]、expand.wgsl
// 通過済み)へ加算合成する(旧luminous/merge.wgsl相当。2パスを1つに統合)。
//   inputTex[0]: 元オブジェクトを拡張キャンバスへ配置したもの(非プリマルチプライド)
//   inputTex[1].a = 累積後の輝度 y_final(既に光色のY / 指定なしでは1.0が入っている)
//   inputTex[2].r = 累積後のCr偏差、inputTex[2].g = 同Cb偏差
//
// 拡散速度0経路では飽和アキュムレータの1枚 {r=Σcr, g=Σcb, a=Σy} を両入力へ
// 渡す(inputTex[1].a=Σy, inputTex[2].r/g=Σcr/Σcb)。拡散速度>0経路では
// finalize_luma(カーブ逆変換後、a=y_final)と finalize_chroma を渡す。
//
// R=Y+1.402Cr, G=Y-0.344136Cb-0.714136Cr, B=Y+1.772Cb。Cb→BとCr/Cb→Gで
// 係数の大きさが非対称なので、色差が輝度より先に飽和したときの色あせ方も
// 非対称になる(実機の黄色寄りの退色)。y_final には既に輝度係数(光色のY、
// 指定なしなら1.0)が入っているので、ここで光色のYを再度掛けてはならない
// (中核(2)の二重掛けバグの修正)。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let dims = vec2<i32>(textureDimensions(inputTex[0]));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let y_final = textureLoad(inputTex[1], coord, 0).a;
    let chroma = textureLoad(inputTex[2], coord, 0);
    let cr = chroma.r;
    let cb = chroma.g;

    let glow_alpha = clamp(y_final, 0.0, 1.0);
    var glow_rgb = vec3<f32>(0.0);
    if (glow_alpha > 1e-6) {
        glow_rgb = bt601_decode(cr, cb, y_final) / glow_alpha;
    }

    // 旧merge.wgsl: 両者をプリマルチプライド形式に変換してから単純加算し、
    // 最後に出力alphaで割って非プリマルチプライドに戻す。実機の合成
    // (sub_10053890)はアルファを**加算**して1.0でクランプする(通常の
    // a+b-a*b ではない)。
    let base = textureLoad(inputTex[0], coord, 0);
    let base_premul = base.rgb * base.a;
    let glow_premul = glow_rgb * glow_alpha;
    let out_alpha = clamp(base.a + glow_alpha, 0.0, 1.0);

    var out_rgb = vec3<f32>(0.0);
    if (out_alpha > 1e-6) {
        out_rgb = clamp((base_premul + glow_premul) / out_alpha, vec3<f32>(0.0), vec3<f32>(1.0));
    }

    textureStore(outputTex, coord, vec4<f32>(out_rgb, out_alpha));
}
