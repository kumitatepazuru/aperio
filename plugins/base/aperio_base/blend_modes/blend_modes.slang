// AviUtl 拡張編集の合成モード13種(exedit-inspect inspect/common/blend_modes.md §3)。
//
// モードはコンパイル時の BLEND_MODE で選ぶ(1つのソースから13個のシェーダーを作る)。
// 配置(移動・拡大・回転・透視投影)と、合成色 blend_rgb をフレームへ重ねる末尾の通常合成
// (表Aの Porter-Duff)はマネージャー提供の aperio_blend が行い、ここでは blend_rgb だけを求める。
//
// 値は 1.0 = 実機の 4096(Q12)で読み替えている。YCbCr は BT.601(aperio_color の bt601_encode /
// bt601_decode。実機の係数 22970/5636/11698/29032 と同じ)で、Y は 0〜1、Cb/Cr は ±0.5。
//
// RGB を経由する8モードは、表Aの一部モードにある shift の2ビットずれ(結果が64倍になる実装上の
// 不具合)は再現せず、表Bと同じ素直な式にしている。
//
//   0 = 通常, 1 = 加算, 2 = 減算, 3 = 乗算, 4 = スクリーン, 5 = オーバーレイ, 6 = 比較(明),
//   7 = 比較(暗), 8 = 輝度, 9 = 色差, 10 = 陰影, 11 = 明暗, 12 = 差分
import aperio_blend;
import aperio_color;

#ifndef BLEND_MODE
#define BLEND_MODE 0
#endif

// 加算: Y の上限(実機 8192)と、Cb/Cr が 0 まで落ちきる Y(実機 12288)
static const float ADD_Y_MAX = 2.0;
static const float ADD_CHROMA_ZERO_Y = 3.0;
// 減算: Y が 0 を下回ったとき、Cb/Cr が 0 まで落ちきる深さ(実機 1024)
static const float SUB_CHROMA_FADE = 0.25;
// 乗算・明暗の上限(実機 8192)
static const float RGB_MAX = 2.0;

float3 ycbcr_of(float3 rgb) {
    let ycc = bt601_encode(rgb); // (cr, cb, y)
    return float3(ycc.z, ycc.y, ycc.x);
}

float3 rgb_of(float3 ycbcr) {
    return bt601_decode(ycbcr.z, ycbcr.y, ycbcr.x);
}

// 加算(YCbCr を直接足す)。Y が上限を超えた分だけ色差を薄め、Y は上限で止める。
float3 blend_add(float3 d, float3 s) {
    var ycc = ycbcr_of(d) + ycbcr_of(s);
    if (ycc.x > ADD_Y_MAX) {
        let fade = saturate(ADD_CHROMA_ZERO_Y - ycc.x);
        ycc.yz *= fade;
        ycc.x = ADD_Y_MAX;
    }
    return rgb_of(ycc);
}

// 減算(YCbCr を直接引く)。Y が負になった分だけ色差を薄め、Y は 0 で止める。
float3 blend_subtract(float3 d, float3 s) {
    var ycc = ycbcr_of(d) - ycbcr_of(s);
    if (ycc.x < 0.0) {
        let fade = saturate((ycc.x + SUB_CHROMA_FADE) / SUB_CHROMA_FADE);
        ycc.yz *= fade;
        ycc.x = 0.0;
    }
    return rgb_of(ycc);
}

float3 blend_overlay(float3 d, float3 s) {
    let low = 2.0 * d * s;
    let high = 2.0 * (d + s) - 1.0 - 2.0 * d * s;
    return select(d < 0.5, low, high);
}

// 輝度: Y だけを src のものにし、Cb/Cr は dst のまま
float3 blend_luminance(float3 d, float3 s) {
    return rgb_of(float3(ycbcr_of(s).x, ycbcr_of(d).yz));
}

// 色差: Cb/Cr だけを src のものにし、Y は dst のまま
float3 blend_chroma(float3 d, float3 s) {
    return rgb_of(float3(ycbcr_of(d).x, ycbcr_of(s).yz));
}

float3 blend_mode_rgb(float3 d, float3 s) {
    switch (BLEND_MODE) {
        case 1: return blend_add(d, s);
        case 2: return blend_subtract(d, s);
        case 3: return min(d * s, RGB_MAX);                 // 乗算
        case 4: return d + s - d * s;                       // スクリーン
        case 5: return blend_overlay(d, s);
        case 6: return max(d, s);                           // 比較(明)
        case 7: return min(d, s);                           // 比較(暗)
        case 8: return blend_luminance(d, s);
        case 9: return blend_chroma(d, s);
        case 10: return max(d + s - 1.0, 0.0);              // 陰影(Linear Burn)
        case 11: return clamp(d + 2.0 * s - 1.0, 0.0, RGB_MAX); // 明暗(Linear Light)
        case 12: return abs(d - s);                         // 差分
        default: return s;                                  // 通常
    }
}

[shader("vertex")]
BlendVaryings vs_main(uint vertex_id : SV_VertexID) {
    return blend_vertex(vertex_id);
}

[shader("fragment")]
float4 fs_main(BlendVaryings v) : SV_Target {
    BlendFragment f;
    if (!blend_fetch(v, f)) {
        discard;
    }
    // 輝度・色差も含め、合成色を求めてから共通の末尾で重ねる。末尾は RGB の線形補間なので、
    // 輝度・色差で dst から引き継いだ面(Cb/Cr または Y)は不透明度に関係なく dst のまま残る。
    return blend_composite(f, blend_mode_rgb(f.dst.rgb, f.src.rgb));
}
