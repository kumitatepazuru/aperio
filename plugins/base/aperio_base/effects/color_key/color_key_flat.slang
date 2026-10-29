import aperio_color;
import common;
import aperio_bindings;

struct ColorKeyFlatParams {
    float key_y;
    float key_cb;
    float key_cr;
    float luma_range;
    float chroma_range;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<ColorKeyFlatParams>> res;

// `境界補正 = 0` の経路(exedit-inspect color_key README §4)。イン・プレースで
// 1回のワーカーが走るだけの実機と同じく、単一パスで完結する。rgbは一切変更しない。
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

    let alpha = key_alpha(y, cb, cr, res.params[0].key_y, res.params[0].key_cb, res.params[0].key_cr, res.params[0].luma_range, res.params[0].chroma_range, src.a);

    res.outputTex[coord] = float4(src.rgb, alpha);
}
