import aperio_color;
import aperio_bindings;

struct FlipParams {
    int flip_v;
    int flip_h;
    int invert_luma;
    int invert_chroma;
    int invert_alpha;
    int _pad1;
    int _pad2;
    int _pad3;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<FlipParams>> res;

[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let out_coord = int2(global_id.xy);
    let tex = inputs.tex0;
    uint dimsU, dimsV;
    tex.GetDimensions(dimsU, dimsV);
    let in_dims = int2(dimsU, dimsV);

    if (out_coord.x >= in_dims.x || out_coord.y >= in_dims.y) {
        return;
    }

    var in_x = out_coord.x;
    var in_y = out_coord.y;

    if (res.params[0].flip_h != 0) {
        in_x = in_dims.x - 1 - in_x;
    }
    if (res.params[0].flip_v != 0) {
        in_y = in_dims.y - 1 - in_y;
    }

    var color = tex.Load(int3(in_x, in_y, 0));

    // 輝度・色相反転
    if (res.params[0].invert_luma != 0 || res.params[0].invert_chroma != 0) {
        var ycbcr = bt601_encode(color.rgb);
        // ycbcr: float3(cr, cb, y)
        if (res.params[0].invert_luma != 0) {
            ycbcr.z = 1.0 - ycbcr.z;
        }
        if (res.params[0].invert_chroma != 0) {
            ycbcr.x = -ycbcr.x;
            ycbcr.y = -ycbcr.y;
        }
        color = float4(bt601_decode(ycbcr.x, ycbcr.y, ycbcr.z), color.a);
    }

    // 透明度反転
    if (res.params[0].invert_alpha != 0) {
        color.a = 1.0 - color.a;
    }

    res.outputTex[out_coord] = color;
}
