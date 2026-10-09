module aperio_math;

// 中心を基準にしたブロック境界を求めるための負数対応floor除算
public int floor_div(int a, int b) {
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
public float curve_forward(float base, float x) {
    return pow(base, x * 256.0) - 1.0;
}

public float curve_inverse(float base, float x) {
    return log(max(x + 1.0, 1e-6)) / (256.0 * log(base));
}

// テクスチャtexを、tex空間の実数座標srcで4点バイリニア補間する
// (sampler不要、Load 4点)。端は最近傍にクランプする。
public float4 bilinear4_load(Texture2D<float4> tex, float2 src) {
    uint dimsU, dimsV;
    tex.GetDimensions(dimsU, dimsV);
    let dims = int2(dimsU, dimsV);
    let s0 = floor(src);
    let frac = src - s0;
    let i0 = int2(s0);
    let i1 = i0 + int2(1, 1);

    let lo = int2(0, 0);
    let hi = dims - int2(1, 1);
    let c00 = tex.Load(int3(clamp(int2(i0.x, i0.y), lo, hi), 0));
    let c10 = tex.Load(int3(clamp(int2(i1.x, i0.y), lo, hi), 0));
    let c01 = tex.Load(int3(clamp(int2(i0.x, i1.y), lo, hi), 0));
    let c11 = tex.Load(int3(clamp(int2(i1.x, i1.y), lo, hi), 0));

    let top = lerp(c00, c10, frac.x);
    let bottom = lerp(c01, c11, frac.x);
    return lerp(top, bottom, frac.y);
}

public float4 load_or_zero(Texture2D<float4> tex, int x, int y, int2 dims) {
    if (x < 0 || y < 0 || x >= dims.x || y >= dims.y) {
        return float4(0.0, 0.0, 0.0, 0.0);
    }
    return tex.Load(int3(x, y, 0));
}

// bilinear4_load と同じだが、範囲外テクセルは重み0(透明)として扱う(端をクランプしない)。
public float4 bilinear4_load_transparent(Texture2D<float4> tex, float2 src) {
    uint dimsU, dimsV;
    tex.GetDimensions(dimsU, dimsV);
    let dims = int2(dimsU, dimsV);
    let s0 = floor(src);
    let frac = src - s0;
    let i0 = int2(s0);
    let i1 = i0 + int2(1, 1);

    let c00 = load_or_zero(tex, i0.x, i0.y, dims);
    let c10 = load_or_zero(tex, i1.x, i0.y, dims);
    let c01 = load_or_zero(tex, i0.x, i1.y, dims);
    let c11 = load_or_zero(tex, i1.x, i1.y, dims);

    let top = lerp(c00, c10, frac.x);
    let bottom = lerp(c01, c11, frac.x);
    return lerp(top, bottom, frac.y);
}
