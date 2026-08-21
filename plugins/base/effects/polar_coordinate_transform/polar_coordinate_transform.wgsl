enable wgpu_binding_array;

const PI: f32 = 3.14159265358979;
const SQRT2: f32 = sqrt(2.0);
const MAX_SAMPLES: i32 = 128;

struct PolarParams {
    center_hole: i32,
    r_max: f32,
    rotation_rad: f32,
    swirl_rate: f32,
    out_width: i32,
    out_height: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params_array: PolarParams;

fn wrap_i(v: i32, n: i32) -> i32 {
    return ((v % n) + n) % n;
}

fn wrap_mod_f(x: f32, m: f32) -> f32 {
    return x - floor(x / m) * m;
}

// 行方向(半径)はクランプ、列方向(角度)はwrapする専用バイリニア
// (共通の bilinear4_load はエッジをクランプするのみで列方向のwrapができない
// ため、このエフェクトだけの需要=plugin.md rule 4に従いローカルに定義する)。
fn bilinear4_load_wrapx(tex: texture_2d<f32>, src: vec2<f32>) -> vec4<f32> {
    let dims = vec2<i32>(textureDimensions(tex));
    let s0 = floor(src);
    let frac = src - s0;
    let i0x = wrap_i(i32(s0.x), dims.x);
    let i1x = wrap_i(i32(s0.x) + 1, dims.x);
    let i0y = clamp(i32(s0.y), 0, dims.y - 1);
    let i1y = clamp(i32(s0.y) + 1, 0, dims.y - 1);
    let c00 = textureLoad(tex, vec2<i32>(i0x, i0y), 0);
    let c10 = textureLoad(tex, vec2<i32>(i1x, i0y), 0);
    let c01 = textureLoad(tex, vec2<i32>(i0x, i1y), 0);
    let c11 = textureLoad(tex, vec2<i32>(i1x, i1y), 0);
    return mix(mix(c00, c10, frac.x), mix(c01, c11, frac.x), frac.y);
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let out_coord = vec2<i32>(global_id.xy);
    let out_dims = vec2<i32>(params_array.out_width, params_array.out_height);
    if (out_coord.x >= out_dims.x || out_coord.y >= out_dims.y) {
        return;
    }

    let R = params_array.r_max;
    let dx = f32(out_coord.x) - R;
    let dy = f32(out_coord.y) - R;
    let r = sqrt(dx * dx + dy * dy);

    let tex = inputTex[0];
    let src_dims = vec2<i32>(textureDimensions(tex));
    let src_w = f32(src_dims.x);
    let center_hole_f = f32(params_array.center_hole);

    let src_row = r * (center_hole_f + f32(src_dims.y)) / R - center_hole_f;
    if (src_row < 0.0 || src_row >= f32(src_dims.y)) {
        textureStore(outputTex, out_coord, vec4<f32>(0.0, 0.0, 0.0, 0.0));
        return;
    }

    let theta = params_array.rotation_rad + (R - r) * params_array.swirl_rate - atan2(dx, dy);
    let src_col = wrap_mod_f(theta * src_w / (2.0 * PI), src_w);

    // 適応的スーパーサンプリング: 出力1画素が源画像の角度方向に何画素分を覆うかを
    // 見積もり、覆う量が1画素を超える(=縮小方向でエイリアシングしうる)場合のみ
    // 角度方向に複数タップして平均する(半径方向は追加サンプリングしない)。
    let swirl_extra = min(abs(params_array.swirl_rate) * src_w * SQRT2 / PI, src_w);
    var samples: f32;
    if (r >= 1.0) {
        samples = SQRT2 * src_w / (2.0 * PI * r) + swirl_extra;
    } else {
        samples = SQRT2 * src_w / (2.0 * PI) + swirl_extra;
    }

    if (samples <= 1.0) {
        textureStore(outputTex, out_coord, bilinear4_load_wrapx(tex, vec2<f32>(src_col, src_row)));
        return;
    }

    let n = clamp(i32(round(samples)), 2, MAX_SAMPLES);
    var sum_rgb = vec3<f32>(0.0, 0.0, 0.0);
    var sum_a = 0.0;
    for (var k = 0; k < n; k = k + 1) {
        let offset = f32(k) - f32(n - 1) * 0.5;
        let col = wrap_mod_f(src_col + offset, src_w);
        let c = bilinear4_load_wrapx(tex, vec2<f32>(col, src_row));
        sum_rgb += c.rgb * c.a;
        sum_a += c.a;
    }
    var out_rgb = vec3<f32>(0.0, 0.0, 0.0);
    if (sum_a > 1e-6) {
        out_rgb = sum_rgb / sum_a;
    }
    textureStore(outputTex, out_coord, vec4<f32>(out_rgb, sum_a / f32(n)));
}
