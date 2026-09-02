import aperio_color;
import aperio_bindings;

struct LumaKeyParams {
    float base;
    float blur;
    int key_type;
};

struct InputTextures { Texture2D<float4> tex0; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<LumaKeyParams>> res;

// exedit-inspect luma_key README §7: 近傍を一切見ず、その画素自身のy・aだけで
// アルファを決める単一パス(境界補正に相当する処理は実機に存在しない)。
// 読むのはyだけ、書くのはaだけでrgbは素通りする。
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
    let y = bt601_luma(src.rgb);

    // README §4: type0/1は帯の片側全部が通過域になるランプ、type2/3は
    // 折り返し(t>blurならt=2*blur-t)により閉じた式 blur-|y-base| に一致する。
    var t: float;
    if (res.params[0].key_type == 0) {
        t = y - res.params[0].base + res.params[0].blur;
    } else if (res.params[0].key_type == 1) {
        t = res.params[0].base + res.params[0].blur - y;
    } else {
        t = res.params[0].blur - abs(y - res.params[0].base);
    }

    // README §4: 境界は「残す側」が閉じている(t<0->0、t>=blurはそのまま)。
    // type3だけランプ枝を持たない(乗除算なしのハードバンド)。
    var alpha: float = src.a;
    if (t < 0.0) {
        alpha = 0.0;
    } else if (res.params[0].key_type != 3 && t < res.params[0].blur) {
        alpha = src.a * (t / res.params[0].blur);
    }

    res.outputTex[coord] = float4(src.rgb, alpha);
}
