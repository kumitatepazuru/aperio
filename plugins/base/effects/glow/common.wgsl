#define_import_path aperio::glow

// 方向ベクトルに沿ったボックス**和**(alpha非加重、範囲外はゼロ埋め)。
// box_average_dir.wgsl(固定幅で割って平均化)とline_accumulate.wgsl(gain倍して
// 蓄積に加算)の両方が同じループを使うため、ここに共通化してある。
fn directional_box_sum(tex: texture_2d<f32>, coord: vec2<i32>, step: vec2<i32>, radius: i32) -> vec3<f32> {
    let in_dims = vec2<i32>(textureDimensions(tex));
    var sum_rgb = vec3<f32>(0.0);
    for (var k = -radius; k <= radius; k++) {
        let sc = coord + step * k;
        if (sc.x >= 0 && sc.x < in_dims.x && sc.y >= 0 && sc.y < in_dims.y) {
            sum_rgb += textureLoad(tex, sc, 0).rgb;
        }
    }
    return sum_rgb;
}
