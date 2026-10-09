import aperio_color;
import common;
import aperio_bindings;

struct ChromaKeyBorderPass1Params {
    float key_cb;
    float key_cr;
    float key_sat;
    float hue_range_turns;
    float sat_range;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<ChromaKeyBorderPass1Params>> res;

// `境界補正 > 0` のパス1(exedit-inspect chroma_key README §7)。
// map_b(= clamp(d, 4096) を 1.0 基準に正規化したもの)と map_c(= hue_excess、
// 色彩補正専用)を .rg に詰めて出力する。`色彩補正` の有無に関わらず両方を常に
// 計算する(値を使うかどうかは後段パス3の分岐が決めるので、ここでは分岐不要)。
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

    let metrics = key_metrics(cb, cr, res.params[0].key_cb, res.params[0].key_cr, res.params[0].key_sat, res.params[0].hue_range_turns, res.params[0].sat_range);
    let map_b = clamp(metrics.d_norm, 0.0, 1.0);
    let map_c = metrics.hue_excess;

    res.outputTex[coord] = float4(map_b, map_c, 0.0, 0.0);
}
