#define_import_path aperio::math

// 中心を基準にしたブロック境界を求めるための負数対応floor除算
fn floor_div(a: i32, b: i32) -> i32 {
    var q = a / b;
    let r = a % b;
    if (r != 0 && ((r < 0) != (b < 0))) {
        q = q - 1;
    }
    return q;
}

// べき平均カーブの往復変換。ぼかし前に対象量(0~1程度の値)を
// pow(base, r*256)-1 で指数的に持ち上げ、ぼかし後に対数で戻す
// (ちょうど逆関数になっている)。baseが1より大きいほど、ぼかしカーネル内で
// 最も大きい値の寄与が単純平均より強く効くようになる。
fn curve_forward(base: f32, x: f32) -> f32 {
    return pow(base, x * 256.0) - 1.0;
}

fn curve_inverse(base: f32, x: f32) -> f32 {
    return log(max(x + 1.0, 1e-6)) / (256.0 * log(base));
}

// テクスチャtexを、tex空間の実数座標srcで4点バイリニア補間する
// (sampler不要、textureLoad 4点)。端は最近傍にクランプする。
fn bilinear4_load(tex: texture_2d<f32>, src: vec2<f32>) -> vec4<f32> {
    let dims = vec2<i32>(textureDimensions(tex));
    let s0 = floor(src);
    let frac = src - s0;
    let i0 = vec2<i32>(s0);
    let i1 = i0 + vec2<i32>(1, 1);

    let lo = vec2<i32>(0, 0);
    let hi = dims - vec2<i32>(1, 1);
    let c00 = textureLoad(tex, clamp(vec2<i32>(i0.x, i0.y), lo, hi), 0);
    let c10 = textureLoad(tex, clamp(vec2<i32>(i1.x, i0.y), lo, hi), 0);
    let c01 = textureLoad(tex, clamp(vec2<i32>(i0.x, i1.y), lo, hi), 0);
    let c11 = textureLoad(tex, clamp(vec2<i32>(i1.x, i1.y), lo, hi), 0);

    let top = mix(c00, c10, frac.x);
    let bottom = mix(c01, c11, frac.x);
    return mix(top, bottom, frac.y);
}

fn load_or_zero(tex: texture_2d<f32>, x: i32, y: i32, dims: vec2<i32>) -> vec4<f32> {
    if (x < 0 || y < 0 || x >= dims.x || y >= dims.y) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    return textureLoad(tex, vec2<i32>(x, y), 0);
}

// bilinear4_load と同じだが、範囲外テクセルは重み0(透明)として扱う(端をクランプしない)。
fn bilinear4_load_transparent(tex: texture_2d<f32>, src: vec2<f32>) -> vec4<f32> {
    let dims = vec2<i32>(textureDimensions(tex));
    let s0 = floor(src);
    let frac = src - s0;
    let i0 = vec2<i32>(s0);
    let i1 = i0 + vec2<i32>(1, 1);

    let c00 = load_or_zero(tex, i0.x, i0.y, dims);
    let c10 = load_or_zero(tex, i1.x, i0.y, dims);
    let c01 = load_or_zero(tex, i0.x, i1.y, dims);
    let c11 = load_or_zero(tex, i1.x, i1.y, dims);

    let top = mix(c00, c10, frac.x);
    let bottom = mix(c01, c11, frac.x);
    return mix(top, bottom, frac.y);
}
