import aperio_bindings;

static const float PI = 3.14159265358979;
static const float SQRT2 = 1.41421356237309504880;
static const int MAX_SAMPLES = 128;

struct PolarParams {
    int center_hole;
    float r_max;
    float rotation_rad;
    float swirl_rate;
    int out_width;
    int out_height;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<PolarParams>> res;

int wrap_i(int v, int n) {
    return ((v % n) + n) % n;
}

float wrap_mod_f(float x, float m) {
    return x - floor(x / m) * m;
}

// 行方向(半径)はクランプ、列方向(角度)はwrapする専用バイリニア
// (共通の bilinear4_load はエッジをクランプするのみで列方向のwrapができない
// ため、このエフェクトだけの需要=plugin.md rule 4に従いローカルに定義する)。
float4 bilinear4_load_wrapx(Texture2D<float4> tex, float2 src) {
    uint dimsU, dimsV;
    tex.GetDimensions(dimsU, dimsV);
    let dims = int2(dimsU, dimsV);
    let s0 = floor(src);
    let frac = src - s0;
    let i0x = wrap_i(int(s0.x), dims.x);
    let i1x = wrap_i(int(s0.x) + 1, dims.x);
    let i0y = clamp(int(s0.y), 0, dims.y - 1);
    let i1y = clamp(int(s0.y) + 1, 0, dims.y - 1);
    let c00 = tex.Load(int3(i0x, i0y, 0));
    let c10 = tex.Load(int3(i1x, i0y, 0));
    let c01 = tex.Load(int3(i0x, i1y, 0));
    let c11 = tex.Load(int3(i1x, i1y, 0));
    return lerp(lerp(c00, c10, frac.x), lerp(c01, c11, frac.x), frac.y);
}

[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let out_coord = int2(global_id.xy);
    let out_dims = int2(res.params[0].out_width, res.params[0].out_height);
    if (out_coord.x >= out_dims.x || out_coord.y >= out_dims.y) {
        return;
    }

    let R = res.params[0].r_max;
    let dx = float(out_coord.x) - R;
    let dy = float(out_coord.y) - R;
    let r = sqrt(dx * dx + dy * dy);

    let tex = inputs.tex0;
    uint dimsU, dimsV;
    tex.GetDimensions(dimsU, dimsV);
    let src_dims = int2(dimsU, dimsV);
    let src_w = float(src_dims.x);
    let center_hole_f = float(res.params[0].center_hole);

    let src_row = r * (center_hole_f + float(src_dims.y)) / R - center_hole_f;
    if (src_row < 0.0 || src_row >= float(src_dims.y)) {
        res.outputTex[out_coord] = float4(0.0, 0.0, 0.0, 0.0);
        return;
    }

    let theta = -res.params[0].rotation_rad + (R - r) * res.params[0].swirl_rate - atan2(dx, dy);
    let src_col = wrap_mod_f(theta * src_w / (2.0 * PI), src_w);

    // 適応的スーパーサンプリング: 出力1画素が源画像の角度方向に何画素分を覆うかを
    // 見積もり、覆う量が1画素を超える(=縮小方向でエイリアシングしうる)場合のみ
    // 角度方向に複数タップして平均する(半径方向は追加サンプリングしない)。
    let swirl_extra = min(abs(res.params[0].swirl_rate) * src_w * SQRT2 / PI, src_w);
    var samples: float;
    if (r >= 1.0) {
        samples = SQRT2 * src_w / (2.0 * PI * r) + swirl_extra;
    } else {
        samples = SQRT2 * src_w / (2.0 * PI) + swirl_extra;
    }

    if (samples <= 1.0) {
        res.outputTex[out_coord] = bilinear4_load_wrapx(tex, float2(src_col, src_row));
        return;
    }

    let n = clamp(int(round(samples)), 2, MAX_SAMPLES);
    var sum_rgb: float3 = float3(0.0, 0.0, 0.0);
    var sum_a: float = 0.0;
    for (int k = 0; k < n; k++) {
        let offset = float(k) - float(n - 1) * 0.5;
        let col = wrap_mod_f(src_col + offset, src_w);
        let c = bilinear4_load_wrapx(tex, float2(col, src_row));
        sum_rgb += c.rgb * c.a;
        sum_a += c.a;
    }
    var out_rgb: float3 = float3(0.0, 0.0, 0.0);
    if (sum_a > 1e-6) {
        out_rgb = sum_rgb / sum_a;
    }
    res.outputTex[out_coord] = float4(out_rgb, sum_a / float(n));
}
