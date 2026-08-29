enable wgpu_binding_array;

struct ResizeDownParams {
    src_width: i32,
    src_height: i32,
    out_width: i32,
    out_height: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: ResizeDownParams;

// アルファ加重の面積平均によるダウンサンプル(exedit-inspect lens_blur README §4
// 「(4)/(7) リサイズ」)。出力画素ごとに対応する入力矩形(scale = src/dst)を求め、
// その矩形に「中心」が収まる入力テクセルだけを単純平均する ―― 部分被覆の面積
// 按分(サブピクセルカバレッジ)までは行わないニアレストネイバー矩形サンプリング
// による簡略化。curve_forward後のカーブ空間の値をそのまま扱うため、このシェーダー
// は32bit float必須(disc_blur.wgslと同じ理由)。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let out_coord = vec2<i32>(global_id.xy);

    if (out_coord.x >= params.out_width || out_coord.y >= params.out_height) {
        return;
    }

    let tex = inputTex[0];
    let src_dims = vec2<i32>(params.src_width, params.src_height);
    let scale = vec2<f32>(src_dims) / vec2<f32>(f32(params.out_width), f32(params.out_height));

    let rect_lo = vec2<f32>(out_coord) * scale;
    let rect_hi = vec2<f32>(out_coord + vec2<i32>(1, 1)) * scale;

    let ix_lo = clamp(i32(floor(rect_lo.x)), 0, src_dims.x - 1);
    let ix_hi = clamp(i32(ceil(rect_hi.x)), 0, src_dims.x - 1);
    let iy_lo = clamp(i32(floor(rect_lo.y)), 0, src_dims.y - 1);
    let iy_hi = clamp(i32(ceil(rect_hi.y)), 0, src_dims.y - 1);

    var sum_rgb = vec3<f32>(0.0);
    var sum_a = 0.0;
    var count = 0;

    for (var sy = iy_lo; sy <= iy_hi; sy++) {
        for (var sx = ix_lo; sx <= ix_hi; sx++) {
            let center = vec2<f32>(f32(sx) + 0.5, f32(sy) + 0.5);
            if (center.x >= rect_lo.x && center.x < rect_hi.x && center.y >= rect_lo.y && center.y < rect_hi.y) {
                let s = textureLoad(tex, vec2<i32>(sx, sy), 0);
                let a = max(s.a, 0.0);
                sum_rgb += s.rgb * a;
                sum_a += a;
                count++;
            }
        }
    }

    if (count == 0) {
        // 矩形が1テクセルの中心すら含まない縮退ケース(通常は起きないが安全策
        // として矩形中心に最も近いテクセルへフォールバックする)。
        let fx = clamp(i32(floor((rect_lo.x + rect_hi.x) * 0.5)), 0, src_dims.x - 1);
        let fy = clamp(i32(floor((rect_lo.y + rect_hi.y) * 0.5)), 0, src_dims.y - 1);
        textureStore(outputTex, out_coord, textureLoad(tex, vec2<i32>(fx, fy), 0));
        return;
    }

    var out_rgb = vec3<f32>(0.0);
    if (sum_a > 1e-6) {
        out_rgb = sum_rgb / sum_a;
    }
    let out_a = sum_a / f32(count);

    textureStore(outputTex, out_coord, vec4<f32>(out_rgb, out_a));
}
