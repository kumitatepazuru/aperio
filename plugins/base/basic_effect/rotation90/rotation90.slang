import aperio_bindings;

struct Rotation90Params {
    int mode;
    int out_width;
    int out_height;
};

struct InputTextures { Texture2D<float4> tex0; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<Rotation90Params>> res;

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

    var in_coord: int2;
    if (res.params[0].mode == 1) {
        // 90° 時計回り
        in_coord = int2(out_coord.y, in_dims.y - 1 - out_coord.x);
    } else if (res.params[0].mode == 2) {
        // 180°
        in_coord = int2(in_dims.x - 1 - out_coord.x, in_dims.y - 1 - out_coord.y);
    } else if (res.params[0].mode == 3) {
        // 270° 時計回り (90° 反時計回り)
        in_coord = int2(in_dims.x - 1 - out_coord.y, out_coord.x);
    } else {
        in_coord = out_coord;
    }

    res.outputTex[out_coord] = tex.Load(int3(in_coord, 0));
}
