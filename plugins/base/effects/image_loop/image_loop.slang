import aperio_bindings;

struct ImageLoopParams {
    int tile_w;
    int tile_h;
    int offset_x;
    int offset_y;
    int out_width;
    int out_height;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<ImageLoopParams>> res;

[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let out_coord = int2(global_id.xy);
    let out_dims = int2(res.params[0].out_width, res.params[0].out_height);
    if (out_coord.x >= out_dims.x || out_coord.y >= out_dims.y) {
        return;
    }

    let tile_dims = int2(res.params[0].tile_w, res.params[0].tile_h);
    let shifted = (out_coord % tile_dims) + int2(res.params[0].offset_x, res.params[0].offset_y);
    let src_coord = ((shifted % tile_dims) + tile_dims) % tile_dims;

    res.outputTex[out_coord] = inputs.tex0.Load(int3(src_coord, 0));
}
