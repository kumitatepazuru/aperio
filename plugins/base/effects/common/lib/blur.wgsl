#define_import_path aperio::blur

struct BoxSumResult {
    sum_rgb: vec3<f32>,
    sum_weight: f32,
    valid_count: i32,
};

// 一様重みのボックス(矩形)ぼかしの、方向ベクトルに沿ったアルファ加重サンプル和。
// box_blur_dir.wgsl(rgb+alphaのぼかし本体)と boundary_blur/box_blur_h_alpha_merge.wgsl
// (alphaだけを使う融合シェーダー)の両方が読む共通ループ。
//
// border_mode:
//   0: 端をクランプして常にサンプルする(luminousの近似ぼかし用、挙動不変)。
//   1: 範囲外は寄与ゼロ(ぼかしフィルタの実機挙動、README手順4)。
fn box_sum(tex: texture_2d<f32>, in_coord: vec2<i32>, step: vec2<i32>, radius: i32, border_mode: i32) -> BoxSumResult {
    let in_dims = vec2<i32>(textureDimensions(tex));

    var sum_rgb = vec3<f32>(0.0);
    var sum_weight = 0.0;
    var valid_count = 0;

    for (var k = -radius; k <= radius; k++) {
        let raw = in_coord + step * k;
        if (border_mode == 0) {
            let sc = clamp(raw, vec2<i32>(0, 0), in_dims - vec2<i32>(1, 1));
            let s = textureLoad(tex, sc, 0);
            let w = max(s.a, 0.0);
            sum_rgb += s.rgb * w;
            sum_weight += w;
            valid_count++;
        } else if (raw.x >= 0 && raw.x < in_dims.x && raw.y >= 0 && raw.y < in_dims.y) {
            let s = textureLoad(tex, raw, 0);
            let w = max(s.a, 0.0);
            sum_rgb += s.rgb * w;
            sum_weight += w;
            valid_count++;
        }
        // border_mode==1で範囲外: 寄与ゼロ(有効サンプル数にも数えない)
    }

    return BoxSumResult(sum_rgb, sum_weight, valid_count);
}

// 一様重みのボックス(矩形)ぼかしの、方向ベクトルに沿ったalpha非加重サンプル和。
// 範囲外はゼロ埋め(寄与ゼロ)。box_sum と違い全チャンネル(rgba)をそのまま合計するので、
// 色以外の任意のスカラー/ベクトル値(グローの発光量、クロマキーのマップなど)を
// 詰めたテクスチャにも使える。common/box_average_dir.wgsl・glow/line_accumulate.wgsl・
// chroma_key の境界補正パスが共通で使う。
fn plain_box_sum(tex: texture_2d<f32>, coord: vec2<i32>, step: vec2<i32>, radius: i32) -> vec4<f32> {
    let in_dims = vec2<i32>(textureDimensions(tex));
    var sum = vec4<f32>(0.0);
    for (var k = -radius; k <= radius; k++) {
        let sc = coord + step * k;
        if (sc.x >= 0 && sc.x < in_dims.x && sc.y >= 0 && sc.y < in_dims.y) {
            sum += textureLoad(tex, sc, 0);
        }
    }
    return sum;
}
