import aperio_color;
import aperio_bindings;

struct GradationParams {
    float cx;
    float cy;
    // angle_vector規約(sin=x, -cos=y)で事前計算済みの単位方向ベクトル。
    float vx;
    float vy;
    float width;
    // 0=線, 1=円, 2=四角形, 3=凸形。
    int shape;
    float start_r;
    float start_g;
    float start_b;
    float start_a;
    float end_r;
    float end_g;
    float end_b;
    float end_a;
    // 0=通常,1=加算,2=減算,3=乗算,4=スクリーン,5=オーバーレイ,6=比較(明),7=比較(暗),
    // 8=輝度,9=色差,10=陰影,11=明暗,12=差分。
    int blend_mode;
    int out_width;
    int out_height;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<GradationParams>> res;

static const float PI = 3.14159265358979;

// exedit-inspect gradation README/blend_modes.md: 13種の合成モードのうち
// ビット単位の式が非公開なもの(陰影/明暗など)は、合成モード
// 解説(Photoshopの定型文と一致することを確認済み)に基づく式で近似する。
// 輝度/色差はRGB経由ではなくYCbCrネイティブなので blend_rgb 側で個別に扱う。
float blend_channel(int mode, float d, float s) {
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

float3 blend_rgb(int mode, float3 dst, float3 src) {
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
    return float3(
        blend_channel(mode, dst.x, src.x),
        blend_channel(mode, dst.y, src.y),
        blend_channel(mode, dst.z, src.z),
    );
}

[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let coord = int2(global_id.xy);
    if (coord.x >= res.params[0].out_width || coord.y >= res.params[0].out_height) {
        return;
    }

    let dst = inputs.tex0.Load(int3(coord, 0));

    let dx = float(coord.x) + 0.5 - res.params[0].cx;
    let dy = float(coord.y) + 0.5 - res.params[0].cy;
    let s_par = res.params[0].vx * dx + res.params[0].vy * dy;
    let s_perp = res.params[0].vy * dx - res.params[0].vx * dy;

    var u: float;
    if (res.params[0].shape == 1) {
        u = res.params[0].width - sqrt(dx * dx + dy * dy);
    } else if (res.params[0].shape == 2) {
        u = res.params[0].width - abs(s_par) - abs(s_perp);
    } else if (res.params[0].shape == 3) {
        u = res.params[0].width - abs(s_par);
    } else {
        u = res.params[0].width * 0.5 + s_par;
    }

    let end_color = float4(res.params[0].end_r, res.params[0].end_g, res.params[0].end_b, res.params[0].end_a);
    let start_color = float4(res.params[0].start_r, res.params[0].start_g, res.params[0].start_b, res.params[0].start_a);

    var grad: float4;
    if (u <= 0.0) {
        grad = end_color;
    } else if (u >= res.params[0].width) {
        grad = start_color;
    } else {
        let tt = u / res.params[0].width;
        let e = 0.5 - 0.5 * cos(PI * tt);
        grad = lerp(end_color, start_color, e);
    }

    let blended_rgb = blend_rgb(res.params[0].blend_mode, dst.rgb, grad.rgb);
    let src_a = clamp(grad.a, 0.0, 1.0);

    // exedit-inspect blend_modes.md §3 表B: dstのアルファには一切触れない
    // 単純な線形補間(out_a = a_d 固定)。表A(Porter-Duffのsrc-over)とは
    // dst.a < 1 のとき結果が異なる。
    let out_rgb = dst.rgb + src_a * (blended_rgb - dst.rgb);

    res.outputTex[coord] = float4(out_rgb, dst.a);
}
