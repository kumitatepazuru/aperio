import aperio_color;
import common;
import aperio_bindings;

struct ColorKeyBorderPass1Params {
    float key_y;
    float key_cb;
    float key_cr;
    float luma_range;
    float chroma_range;
};

struct InputTextures { Texture2D<float4> tex0; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<ColorKeyBorderPass1Params>> res;

// `境界補正 > 0` のパス1(exedit-inspect color_key README §5)。キー判定後の
// アルファ(a0 = 直方体内なら0、それ以外は元のアルファ)を .r だけに詰めて出力する。
// chroma_keyのpass1と違い「色彩補正」用の第2マップは無いので1チャンネルのみ。
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

    let a0 = key_alpha(y, cb, cr, res.params[0].key_y, res.params[0].key_cb, res.params[0].key_cr, res.params[0].luma_range, res.params[0].chroma_range, src.a);

    res.outputTex[coord] = float4(a0, 0.0, 0.0, 0.0);
}
