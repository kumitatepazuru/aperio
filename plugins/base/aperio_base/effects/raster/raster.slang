import aperio_bindings;

static const float PI = 3.14159265358979;

struct RasterParams {
    float amplitude_px;
    float wavelength_px;
    float phase;
    int vertical;
    int random_amplitude;
    uint effect_seed;
    int shift_offset;
    int out_width;
    int out_height;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<RasterParams>> res;

uint hash_u32(uint x) {
    uint h = x;
    h = h ^ (h >> 16u);
    h = h * 0x7feb352du;
    h = h ^ (h >> 15u);
    h = h * 0x846ca68bu;
    h = h ^ (h >> 16u);
    return h;
}

float hash_to_unit(uint seed) {
    return float(hash_u32(seed)) * (1.0 / 4294967295.0);
}

uint combine_seed(uint seed, uint o, uint j, int cycle) {
    var h = hash_u32(seed ^ (o * 0x9E3779B1u));
    h = hash_u32(h ^ (j * 0x85EBCA77u));
    h = hash_u32(h ^ asuint(cycle));
    return h;
}

// 出力座標 t(行または列インデックス)における波の変位量。ランダム振幅ONの
// 場合、GPUの並列実行とは相性が悪い逐次疑似乱数(原作)の代わりに、
// (シード, オクターブ, 系統salt, 周期番号)から直接1値を導く純関数ハッシュを
// 3系統×3オクターブ=9項重ね合わせる。原作(exedit-inspect README)の構造に
// 合わせ、3系統(j=0,1,2)は空間周波数ではなく「位相のスクロール速度」のみが
// 異なる(phase, phase/2, phase/4)。空間周波数の倍化はオクターブ(o)のみが
// 担い、重みは系統・オクターブそれぞれで半減する(weight=1/2^(o+j))。
float wave_shift(float t) {
    if (res.params[0].random_amplitude == 0) {
        return res.params[0].amplitude_px * sin(2.0 * PI * (t + res.params[0].phase) / res.params[0].wavelength_px);
    }
    var acc: float = 0.0;
    uint salts[3] = { 0u, 7u, 13u };
    for (uint o = 0; o < 3; o++) {
        let freq_mult = pow(2.0, float(o));
        for (uint j = 0; j < 3; j++) {
            let phase_j = res.params[0].phase / pow(2.0, float(j));
            let t_j = (t + phase_j) * freq_mult;
            let cycle = int(floor(t_j / res.params[0].wavelength_px));
            let seed = combine_seed(res.params[0].effect_seed, o, salts[j], cycle);
            // weight(o,j) = 1/2^(o+j) 、正規化定数は (1+1/2+1/4)^2 = 3.0625
            let weight = 1.0 / pow(2.0, float(o + j));
            acc += (weight * hash_to_unit(seed) / 3.0625) * sin(2.0 * PI * t_j / res.params[0].wavelength_px);
        }
    }
    return res.params[0].amplitude_px * acc;
}

[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let out_coord = int2(global_id.xy);
    let out_dims = int2(res.params[0].out_width, res.params[0].out_height);
    if (out_coord.x >= out_dims.x || out_coord.y >= out_dims.y) {
        return;
    }

    let tex = inputs.tex0;
    uint dimsU, dimsV;
    tex.GetDimensions(dimsU, dimsV);
    let src_dims = int2(dimsU, dimsV);
    let vertical = res.params[0].vertical != 0;

    let t_index = vertical ? out_coord.x : out_coord.y;
    let base_out = vertical ? out_coord.y : out_coord.x;
    let base_src = base_out + res.params[0].shift_offset;
    let t = float(t_index);

    let shift0 = round(wave_shift(t));
    let shift1 = round(wave_shift(t + 1.0));
    let lo = int(min(shift0, shift1));
    let hi = int(max(shift0, shift1));
    let n = hi - lo + 1;
    let src_shift_dim = vertical ? src_dims.y : src_dims.x;

    var sum_premul: float3 = float3(0.0, 0.0, 0.0);
    var sum_alpha: float = 0.0;
    for (int k = lo; k <= hi; k++) {
        let src_shift = base_src + k;
        if (src_shift >= 0 && src_shift < src_shift_dim) {
            let src = vertical ? int2(t_index, src_shift) : int2(src_shift, t_index);
            let c = tex.Load(int3(src, 0));
            sum_premul += c.rgb * c.a;
            sum_alpha += c.a;
        }
    }
    var out_color: float4 = float4(0.0, 0.0, 0.0, 0.0);
    if (sum_alpha > 0.0) {
        out_color = float4(sum_premul / sum_alpha, sum_alpha / float(n));
    }
    res.outputTex[out_coord] = out_color;
}
