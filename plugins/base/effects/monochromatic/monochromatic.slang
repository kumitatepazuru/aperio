import aperio_color;
import aperio_bindings;

struct MonochromaticParams {
    float strength;
    // 1なら輝度(Y)は維持してCb/Crのみターゲット色へ寄せる。0ならYも寄せる。
    int preserve_luminance;
    float color_r;
    float color_g;
    float color_b;
};

struct InputTextures { Texture2D<float4> tex0; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<MonochromaticParams>> res;

// exedit-inspect noise/monochromatic README: 全画素のCb/Crを strength でターゲット色へ
// 線形補間する。preserve_luminance が真ならYは触らない(明暗はそのまま、色味だけが
// ターゲット色に染まる)。alphaは一切読み書きしない。
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
    let src_ycc = bt601_encode(src.rgb);
    let key_ycc = bt601_encode(float3(res.params[0].color_r, res.params[0].color_g, res.params[0].color_b));

    let t = clamp(res.params[0].strength, 0.0, 1.0);
    let cr = lerp(src_ycc.x, key_ycc.x, t);
    let cb = lerp(src_ycc.y, key_ycc.y, t);
    var y: float = src_ycc.z;
    if (res.params[0].preserve_luminance == 0) {
        y = lerp(src_ycc.z, key_ycc.z, t);
    }

    let rgb = bt601_decode(cr, cb, y);
    res.outputTex[coord] = float4(rgb, src.a);
}
