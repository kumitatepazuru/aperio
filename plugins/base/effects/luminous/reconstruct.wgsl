@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

// 輝度アキュムレータ(inputTex[0])と色差アキュムレータ(inputTex[1])を
// BT.601フルレンジの逆変換で合成し、最終色(プリマルチプライドをun-premultiply
// したrgb＋alpha)を復元する。
//   inputTex[0].a = 累積後の輝度 y_final(既に光色のY / 指定なしでは1.0が入っている)
//   inputTex[1].r = 累積後のCr偏差、inputTex[1].g = 同Cb偏差
//
// 拡散速度0経路では飽和アキュムレータの1枚 {r=Σcr, g=Σcb, a=Σy} を両入力へ
// 渡す(inputTex[0].a=Σy, inputTex[1].r/g=Σcr/Σcb)。拡散速度>0経路では
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

    let y_final = textureLoad(inputTex[0], coord, 0).a;
    let chroma = textureLoad(inputTex[1], coord, 0);
    let cr = chroma.r;
    let cb = chroma.g;

    let alpha = clamp(y_final, 0.0, 1.0);
    var rgb = vec3<f32>(0.0);
    if (alpha > 1e-6) {
        let r = y_final + 1.402000 * cr;
        let g = y_final - 0.344136 * cb - 0.714136 * cr;
        let b = y_final + 1.772000 * cb;
        rgb = vec3<f32>(r, g, b) / alpha;
    }

    textureStore(outputTex, coord, vec4<f32>(rgb, alpha));
}
