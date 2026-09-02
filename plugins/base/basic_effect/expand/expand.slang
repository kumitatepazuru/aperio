import aperio_bindings;

struct CanvasExtendParams {
    int offset_x;
    int offset_y;
    int out_width;
    int out_height;
    int fill_mode;
};

struct InputTextures { Texture2D<float4> tex0; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<CanvasExtendParams>> res;

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
    let in_dims = int2(dimsU, dimsV);
    let in_coord = out_coord - int2(res.params[0].offset_x, res.params[0].offset_y);

    if (in_coord.x >= 0 && in_coord.x < in_dims.x && in_coord.y >= 0 && in_coord.y < in_dims.y) {
        res.outputTex[out_coord] = tex.Load(int3(in_coord, 0));
    } else if (res.params[0].fill_mode != 0) {
        let clamped = clamp(in_coord, int2(0, 0), in_dims - int2(1, 1));
        res.outputTex[out_coord] = tex.Load(int3(clamped, 0));
    } else {
        res.outputTex[out_coord] = float4(0.0, 0.0, 0.0, 0.0);
    }
}
