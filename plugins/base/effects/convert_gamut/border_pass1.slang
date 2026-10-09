import aperio_color;
import common;
import aperio_bindings;

struct BorderPass1Params {
    float key_hue;
    float key_sat;
    float hue_range;
    float sat_range;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<BorderPass1Params>> res;

// `境界補正 > 0` のパス1。距離 d(gamut_distanceの結果、0=完全一致・1=無関係)を
// .r だけに詰めて出力する(chroma_keyのmap_bと同じ役割)。
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
    let ycc = bt601_encode(src.rgb);
    let d = gamut_distance(ycc.y, ycc.x, res.params[0].key_hue, res.params[0].key_sat, res.params[0].hue_range, res.params[0].sat_range);

    res.outputTex[coord] = float4(d, 0.0, 0.0, 0.0);
}
