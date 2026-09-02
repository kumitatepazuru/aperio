import aperio_color;
import common;
import aperio_bindings;

struct ChromaKeyFlatParams {
    float key_cb;
    float key_cr;
    float key_sat;
    float hue_range_turns;
    float sat_range;
    int color_correction;
    int alpha_correction;
};

struct InputTextures { Texture2D<float4> tex0; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<ChromaKeyFlatParams>> res;

// `境界補正 = 0` の経路(exedit-inspect chroma_key README §5/§6)。イン・プレースで
// 1回のワーカーが走るだけの実機と同じく、単一パスで完結する。
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

    let metrics = key_metrics(cb, cr, res.params[0].key_cb, res.params[0].key_cr, res.params[0].key_sat, res.params[0].hue_range_turns, res.params[0].sat_range);

    var alpha: float = src.a * clamp(metrics.d_norm, 0.0, 1.0);
    var out_cb: float = cb;
    var out_cr: float = cr;

    if (res.params[0].color_correction != 0) {
        let f = unmix_factor(metrics.hue_excess, metrics.sat, res.params[0].key_sat);
        let unmixed = unmix(cb, cr, res.params[0].key_cb, res.params[0].key_cr, f);
        out_cb = unmixed.x;
        out_cr = unmixed.y;
        if (res.params[0].alpha_correction != 0 && f > 1e-4 && f < 1.0) {
            alpha = alpha * f;
        }
    }

    let rgb = bt601_decode(out_cr, out_cb, y);
    res.outputTex[coord] = float4(rgb, alpha);
}
