import aperio_color;
import aperio_bindings;

struct InputTextures { Texture2D<float4> tex0; Texture2D<float4> tex1; Texture2D<float4> tex2; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResourcesNoParams> res;

// 輝度アキュムレータ(inputs.tex1)と色差アキュムレータ(inputs.tex2)を
// BT.601フルレンジの逆変換で合成し、発光色(プリマルチプライドをun-premultiply
// したrgb＋alpha)を復元したうえで、元オブジェクト(inputs.tex0、expand.slang
// 通過済み)へ加算合成する(旧luminous/merge.wgsl相当。2パスを1つに統合)。
//   inputs.tex0: 元オブジェクトを拡張キャンバスへ配置したもの(非プリマルチプライド)
//   inputs.tex1.a = 累積後の輝度 y_final(既に光色のY / 指定なしでは1.0が入っている)
//   inputs.tex2.r = 累積後のCr偏差、inputs.tex2.g = 同Cb偏差
//
// 拡散速度0経路では飽和アキュムレータの1枚 {r=Σcr, g=Σcb, a=Σy} を両入力へ
// 渡す(inputs.tex1.a=Σy, inputs.tex2.r/g=Σcr/Σcb)。拡散速度>0経路では
// finalize_luma(カーブ逆変換後、a=y_final)と finalize_chroma を渡す。
//
// R=Y+1.402Cr, G=Y-0.344136Cb-0.714136Cr, B=Y+1.772Cb。Cb→BとCr/Cb→Gで
// 係数の大きさが非対称なので、色差が輝度より先に飽和したときの色あせ方も
// 非対称になる(実機の黄色寄りの退色)。y_final には既に輝度係数(光色のY、
// 指定なしなら1.0)が入っているので、ここで光色のYを再度掛けてはならない
// (中核(2)の二重掛けバグの修正)。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    uint dimsU, dimsV;
    inputs.tex0.GetDimensions(dimsU, dimsV);
    let dims = int2(dimsU, dimsV);
    let coord = int2(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let y_final = inputs.tex1.Load(int3(coord, 0)).a;
    let chroma = inputs.tex2.Load(int3(coord, 0));
    let cr = chroma.r;
    let cb = chroma.g;

    let glow_alpha = clamp(y_final, 0.0, 1.0);
    var glow_rgb: float3 = float3(0.0, 0.0, 0.0);
    if (glow_alpha > 1e-6) {
        glow_rgb = bt601_decode(cr, cb, y_final) / glow_alpha;
    }

    // 旧merge.wgsl: 両者をプリマルチプライド形式に変換してから単純加算し、
    // 最後に出力alphaで割って非プリマルチプライドに戻す。実機の合成
    // (sub_10053890)はアルファを**加算**して1.0でクランプする(通常の
    // a+b-a*b ではない)。
    let base = inputs.tex0.Load(int3(coord, 0));
    let base_premul = base.rgb * base.a;
    let glow_premul = glow_rgb * glow_alpha;
    let out_alpha = clamp(base.a + glow_alpha, 0.0, 1.0);

    // ここで[0,1]にクランプしてはいけない: out_alphaが小さい(周囲が透明に近い)ほど
    // このstraight-alpha色は1.0を大きく超えうる(光色の輝度係数が1未満なら常に、例えば
    // 既定のマゼンタで約2.42倍)。README手順6の通り、不透明分岐は「単純加算(クランプなし)」、
    // 透明分岐も na=min(g.y,4096) で割るだけでクランプは無く、最終的なクリップは
    // シーン合成後の出力段(aviutl.exe側、aperioではpost_process.wgsl等)でのみ行われる。
    // ここで早期にクランプすると、後段でout_alphaを掛け直した際の明るさが
    // 「クランプせずout_alphaを掛けてから最後にクランプする」場合と一致しなくなり、
    // out_alphaが小さいオブジェクト(透明背景のテキスト等)だけ不当に暗くなる
    // (out_alpha=1に近い動画オブジェクトではこの割り算が実質no-opなので症状が隠れる)。
    var out_rgb: float3 = float3(0.0, 0.0, 0.0);
    if (out_alpha > 1e-6) {
        out_rgb = (base_premul + glow_premul) / out_alpha;
    }

    res.outputTex[coord] = float4(out_rgb, out_alpha);
}
