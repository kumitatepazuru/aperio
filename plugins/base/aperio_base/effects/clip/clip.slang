import aperio_bindings;

struct ClipParams {
    int clip_left;
    int clip_top;
    int out_width;
    int out_height;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<ClipParams>> res;

[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let out_coord = int2(global_id.xy);
    let out_dims = int2(res.params[0].out_width, res.params[0].out_height);

    if (out_coord.x >= out_dims.x || out_coord.y >= out_dims.y) {
        return;
    }

    let tex = inputs.tex0;
    let in_coord = out_coord + int2(res.params[0].clip_left, res.params[0].clip_top);
    let color = tex.Load(int3(in_coord, 0));
    res.outputTex[out_coord] = color;
}
