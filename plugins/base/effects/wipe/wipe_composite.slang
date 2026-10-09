import aperio_bindings;

struct InputTextures { Texture2D<float4> tex0; Texture2D<float4> tex1; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResourcesNoParams> res;

// 最終合成(exedit-inspect wipe README §7): objAlpha *= patternAlpha。
// inputs.tex0 = 元のオブジェクト、inputs.tex1 = wipe_mask.slang が描いた(そして
// ぼかしを通った)パターンマスク。色(rgb)は一度も読み替えないので、ワイプは
// `フェード`同様アルファを減らすことしかできない。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let tex = inputs.tex0;
    uint dimsU, dimsV;
    tex.GetDimensions(dimsU, dimsV);
    let dims = int2(dimsU, dimsV);
    let coord = int2(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let src = tex.Load(int3(coord, 0));
    let mask = inputs.tex1.Load(int3(coord, 0)).a;

    res.outputTex[coord] = float4(src.rgb, src.a * mask);
}
