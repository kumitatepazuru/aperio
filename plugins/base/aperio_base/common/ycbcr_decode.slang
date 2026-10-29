import aperio_color;
import aperio_bindings;

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResourcesNoParams> res;

// ycbcr_encode.slang の逆変換。{r=Cr偏差, g=Cb偏差, b=Y} からBT.601でRGBを
// 復元する。alphaは素通し。
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

    let s = tex.Load(int3(coord, 0));
    let rgb = bt601_decode(s.r, s.g, s.b);

    res.outputTex[coord] = float4(rgb, s.a);
}
