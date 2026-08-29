enable wgpu_binding_array;

const PI: f32 = 3.14159265358979;

struct RasterParams {
    amplitude_px: f32,
    wavelength_px: f32,
    phase: f32,
    vertical: i32,
    random_amplitude: i32,
    effect_seed: u32,
    shift_offset: i32,
    out_width: i32,
    out_height: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params_array: RasterParams;

fn hash_u32(x: u32) -> u32 {
    var h = x;
    h = h ^ (h >> 16u);
    h = h * 0x7feb352du;
    h = h ^ (h >> 15u);
    h = h * 0x846ca68bu;
    h = h ^ (h >> 16u);
    return h;
}

fn hash_to_unit(seed: u32) -> f32 {
    return f32(hash_u32(seed)) * (1.0 / 4294967295.0);
}

fn combine_seed(seed: u32, o: u32, j: u32, cycle: i32) -> u32 {
    var h = hash_u32(seed ^ (o * 0x9E3779B1u));
    h = hash_u32(h ^ (j * 0x85EBCA77u));
    h = hash_u32(h ^ bitcast<u32>(cycle));
    return h;
}

// 出力座標 t(行または列インデックス)における波の変位量。ランダム振幅ONの
// 場合、GPUの並列実行とは相性が悪い逐次疑似乱数(原作)の代わりに、
// (シード, オクターブ, 系統salt, 周期番号)から直接1値を導く純関数ハッシュを
// 3系統×3オクターブ=9項重ね合わせる。原作(exedit-inspect README)の構造に
// 合わせ、3系統(j=0,1,2)は空間周波数ではなく「位相のスクロール速度」のみが
// 異なる(phase, phase/2, phase/4)。空間周波数の倍化はオクターブ(o)のみが
// 担い、重みは系統・オクターブそれぞれで半減する(weight=1/2^(o+j))。
fn wave_shift(t: f32) -> f32 {
    if (params_array.random_amplitude == 0) {
        return params_array.amplitude_px * sin(2.0 * PI * (t + params_array.phase) / params_array.wavelength_px);
    }
    var acc = 0.0;
    let salts = array<u32, 3>(0u, 7u, 13u);
    for (var o = 0u; o < 3u; o = o + 1u) {
        let freq_mult = pow(2.0, f32(o));
        for (var j = 0u; j < 3u; j = j + 1u) {
            let phase_j = params_array.phase / pow(2.0, f32(j));
            let t_j = (t + phase_j) * freq_mult;
            let cycle = i32(floor(t_j / params_array.wavelength_px));
            let seed = combine_seed(params_array.effect_seed, o, salts[j], cycle);
            // weight(o,j) = 1/2^(o+j) 、正規化定数は (1+1/2+1/4)^2 = 3.0625
            let weight = 1.0 / pow(2.0, f32(o + j));
            acc += (weight * hash_to_unit(seed) / 3.0625) * sin(2.0 * PI * t_j / params_array.wavelength_px);
        }
    }
    return params_array.amplitude_px * acc;
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let out_coord = vec2<i32>(global_id.xy);
    let out_dims = vec2<i32>(params_array.out_width, params_array.out_height);
    if (out_coord.x >= out_dims.x || out_coord.y >= out_dims.y) {
        return;
    }

    let tex = inputTex[0];
    let src_dims = vec2<i32>(textureDimensions(tex));
    let vertical = params_array.vertical != 0;

    let t_index = select(out_coord.y, out_coord.x, vertical);
    let base_out = select(out_coord.x, out_coord.y, vertical);
    let base_src = base_out + params_array.shift_offset;
    let t = f32(t_index);

    let shift0 = round(wave_shift(t));
    let shift1 = round(wave_shift(t + 1.0));
    let lo = i32(min(shift0, shift1));
    let hi = i32(max(shift0, shift1));
    let n = hi - lo + 1;
    let src_shift_dim = select(src_dims.x, src_dims.y, vertical);

    var sum_premul = vec3<f32>(0.0, 0.0, 0.0);
    var sum_alpha = 0.0;
    for (var k = lo; k <= hi; k = k + 1) {
        let src_shift = base_src + k;
        if (src_shift >= 0 && src_shift < src_shift_dim) {
            let src = select(vec2<i32>(src_shift, t_index), vec2<i32>(t_index, src_shift), vertical);
            let c = textureLoad(tex, src, 0);
            sum_premul += c.rgb * c.a;
            sum_alpha += c.a;
        }
    }
    var out_color = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    if (sum_alpha > 0.0) {
        out_color = vec4<f32>(sum_premul / sum_alpha, sum_alpha / f32(n));
    }
    textureStore(outputTex, out_coord, out_color);
}
