module aperio_blur;

public struct BoxSumResult {
    public float3 sum_rgb;
    public float sum_weight;
    public int valid_count;
};

// 一様重みのボックス(矩形)ぼかしの、方向ベクトルに沿ったアルファ加重サンプル和。
// box_blur_dir.slang(rgb+alphaのぼかし本体)と boundary_blur/box_blur_h_alpha_merge.slang
// (alphaだけを使う融合シェーダー)の両方が読む共通ループ。
//
// border_mode:
//   0: 端をクランプして常にサンプルする(luminousの近似ぼかし用、挙動不変)。
//   1: 範囲外は寄与ゼロ(ぼかしフィルタの実機挙動、README手順4)。
public BoxSumResult box_sum(Texture2D<float4> tex, int2 in_coord, int2 step, int radius, int border_mode) {
    uint dimsU, dimsV;
    tex.GetDimensions(dimsU, dimsV);
    let in_dims = int2(dimsU, dimsV);

    var sum_rgb: float3 = float3(0.0, 0.0, 0.0);
    var sum_weight: float = 0.0;
    var valid_count: int = 0;

    for (int k = -radius; k <= radius; k++) {
        let raw = in_coord + step * k;
        if (border_mode == 0) {
            let sc = clamp(raw, int2(0, 0), in_dims - int2(1, 1));
            let s = tex.Load(int3(sc, 0));
            let w = max(s.a, 0.0);
            sum_rgb += s.rgb * w;
            sum_weight += w;
            valid_count++;
        } else if (raw.x >= 0 && raw.x < in_dims.x && raw.y >= 0 && raw.y < in_dims.y) {
            let s = tex.Load(int3(raw, 0));
            let w = max(s.a, 0.0);
            sum_rgb += s.rgb * w;
            sum_weight += w;
            valid_count++;
        }
        // border_mode==1で範囲外: 寄与ゼロ(有効サンプル数にも数えない)
    }

    BoxSumResult result;
    result.sum_rgb = sum_rgb;
    result.sum_weight = sum_weight;
    result.valid_count = valid_count;
    return result;
}

// 一様重みのボックス(矩形)ぼかしの、方向ベクトルに沿ったalpha非加重サンプル和。
// 範囲外はゼロ埋め(寄与ゼロ)。box_sum と違い全チャンネル(rgba)をそのまま合計するので、
// 色以外の任意のスカラー/ベクトル値(グローの発光量、クロマキーのマップなど)を
// 詰めたテクスチャにも使える。common/box_average_dir.slang・glow/line_accumulate.slang・
// chroma_key の境界補正パスが共通で使う。
public float4 plain_box_sum(Texture2D<float4> tex, int2 coord, int2 step, int radius) {
    uint dimsU, dimsV;
    tex.GetDimensions(dimsU, dimsV);
    let in_dims = int2(dimsU, dimsV);
    var sum: float4 = float4(0.0, 0.0, 0.0, 0.0);
    for (int k = -radius; k <= radius; k++) {
        let sc = coord + step * k;
        if (sc.x >= 0 && sc.x < in_dims.x && sc.y >= 0 && sc.y < in_dims.y) {
            sum += tex.Load(int3(sc, 0));
        }
    }
    return sum;
}

public struct BorderStretch {
    // 伸張後の生の値(clampなし)。chroma_keyは色彩補正offのとき、この値を
    // hue_excess相当の代用としてスピル除去の係数計算に再利用する。
    public float t;
    // 呼び出し側が自身のalpha基準値に掛けるだけでよい乗算係数。v<=0またはt<=0なら0.0、
    // それ以外はclamp(t, 0, 1)。
    public float factor;
};

// ボックス平均済みの値(avg_raw)と、そのピクセル自身の生値(raw)の積から
// コントラスト伸張を行う(chroma_key/color_key 共通の境界補正伸張式、
// exedit-inspect chroma_key README §7 / color_key README §5)。
public BorderStretch border_stretch(float avg_raw, float raw, float radius, float a_const) {
    let v = avg_raw * raw;
    let t = (v - a_const) * radius;
    var factor: float = clamp(t, 0.0, 1.0);
    if (v <= 0.0 || t <= 0.0) {
        factor = 0.0;
    }
    BorderStretch result;
    result.t = t;
    result.factor = factor;
    return result;
}
