import aperio_bindings;

struct ResizeParams {
    int out_width;
    int out_height;
    int no_interpolation;
    int _pad;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<ResizeParams>> res;

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

    if (res.params[0].no_interpolation != 0) {
        // 最近傍補間 (Nearest Neighbor)
        let in_x = clamp((out_coord.x * in_dims.x) / out_dims.x, 0, in_dims.x - 1);
        let in_y = clamp((out_coord.y * in_dims.y) / out_dims.y, 0, in_dims.y - 1);
        res.outputTex[out_coord] = tex.Load(int3(in_x, in_y, 0));
    } else {
        // バイリニア補間 (Bilinear Interpolation)
        let u = (float(out_coord.x) + 0.5) * float(in_dims.x) / float(out_dims.x) - 0.5;
        let v = (float(out_coord.y) + 0.5) * float(in_dims.y) / float(out_dims.y) - 0.5;

        let x0 = clamp(int(floor(u)), 0, in_dims.x - 1);
        let y0 = clamp(int(floor(v)), 0, in_dims.y - 1);
        let x1 = clamp(x0 + 1, 0, in_dims.x - 1);
        let y1 = clamp(y0 + 1, 0, in_dims.y - 1);

        let fx = clamp(u - floor(u), 0.0, 1.0);
        let fy = clamp(v - floor(v), 0.0, 1.0);

        let c00 = tex.Load(int3(x0, y0, 0));
        let c10 = tex.Load(int3(x1, y0, 0));
        let c01 = tex.Load(int3(x0, y1, 0));
        let c11 = tex.Load(int3(x1, y1, 0));

        let top = lerp(c00, c10, fx);
        let bottom = lerp(c01, c11, fx);
        let color = lerp(top, bottom, fy);

        res.outputTex[out_coord] = color;
    }
}
