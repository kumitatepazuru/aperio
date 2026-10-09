import aperio_color;
import common;
import aperio_bindings;

struct FlatParams {
    float key_hue;
    float key_sat;
    float key_y;
    float after_cr;
    float after_cb;
    float after_y;
    float hue_range;
    float sat_range;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<FlatParams>> res;

// `境界補正 = 0` の経路。単一パスで完結する。
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
    let cr = ycc.x;
    let cb = ycc.y;
    let y = ycc.z;

    let d = gamut_distance(cb, cr, res.params[0].key_hue, res.params[0].key_sat, res.params[0].hue_range, res.params[0].sat_range);
    let out_ycc = gamut_apply(cr, cb, y, res.params[0].key_sat, res.params[0].key_y, res.params[0].after_cr, res.params[0].after_cb, res.params[0].after_y, d);
    let rgb = bt601_decode(out_ycc.x, out_ycc.y, out_ycc.z);

    res.outputTex[coord] = float4(rgb, src.a);
}
