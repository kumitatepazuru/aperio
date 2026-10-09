import aperio_bindings;

struct AccumulateLumaCombinedParams {
    int is_first;
};

struct InputTextures { Texture2D<float4> tex0; Texture2D<float4> tex1; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<AccumulateLumaCombinedParams>> res;

// 輝度・色差の6パスを1本化した経路(combined_init.slang)用の輝度アキュムレータ。
// inputs.tex0: このパスでぼかし済みのチェーン(.bに輝度量、カーブ適用後)
// inputs.tex1: これまでの輝度蓄積(.aに量の合計。is_first!=0のときは読まない)
//
// accumulate_luma.slang(輝度・色差を分けて回す経路用)と同じくクランプなし
// の単純加算(結合則が成り立つため、都度足し込む方式で最後にまとめて
// 加算するのと数学的に同じ)。読み出すチャンネルが.bになる点だけが異なる。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    uint dimsU, dimsV;
    inputs.tex0.GetDimensions(dimsU, dimsV);
    let dims = int2(dimsU, dimsV);
    let coord = int2(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let contribution = inputs.tex0.Load(int3(coord, 0)).b;

    var old_accum: float = 0.0;
    if (res.params[0].is_first == 0) {
        old_accum = inputs.tex1.Load(int3(coord, 0)).a;
    }

    res.outputTex[coord] = float4(0.0, 0.0, 0.0, contribution + old_accum);
}
