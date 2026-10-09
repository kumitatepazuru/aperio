import aperio_bindings;

struct MirrorParams {
    int axis;
    int orig_start;
    int dim;
    int mirror_start;
    int mirror_len;
    int mirror_a;
    int dist_c;
    int dist_slope;
    float base_opacity;
    float falloff_coef;
    int out_width;
    int out_height;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<MirrorParams>> res;

[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let out_coord = int2(global_id.xy);
    let out_dims = int2(res.params[0].out_width, res.params[0].out_height);
    if (out_coord.x >= out_dims.x || out_coord.y >= out_dims.y) {
        return;
    }

    let vertical = res.params[0].axis == 0;
    let t = vertical ? out_coord.y : out_coord.x;
    let s = vertical ? out_coord.x : out_coord.y;
    let tex = inputs.tex0;

    if (t >= res.params[0].orig_start && t < res.params[0].orig_start + res.params[0].dim) {
        let src_axis = t - res.params[0].orig_start;
        let src = vertical ? int2(s, src_axis) : int2(src_axis, s);
        res.outputTex[out_coord] = tex.Load(int3(src, 0));
        return;
    }
    if (t >= res.params[0].mirror_start && t < res.params[0].mirror_start + res.params[0].mirror_len) {
        let src_axis = res.params[0].mirror_a - t;
        let src = vertical ? int2(s, src_axis) : int2(src_axis, s);
        let dist = float(res.params[0].dist_c + res.params[0].dist_slope * t);
        let opacity = clamp(res.params[0].base_opacity - res.params[0].falloff_coef * dist, 0.0, 1.0);
        var color = tex.Load(int3(src, 0));
        color.a = color.a * opacity;
        res.outputTex[out_coord] = color;
        return;
    }
    res.outputTex[out_coord] = float4(0.0, 0.0, 0.0, 0.0);
}
