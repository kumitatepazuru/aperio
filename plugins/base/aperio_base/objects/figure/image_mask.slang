import aperio_bindings;
import aperio_color;

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResourcesNoParams> res;

float luma_at(Texture2D<float4> tex, int2 coord) {
    return bt601_luma(tex.Load(int3(coord, 0)).rgb);
}

// 画像の図形: 画像のアルファも色も捨てて、明るさだけを被覆率(.a)にする。
// 四隅4画素の輝度の和が 2.0 未満(平均が50%未満 = 背景が暗い)なら輝度そのもの
// (白い部分が図形)、それ以外(背景が明るい)なら 1 - 輝度(黒い部分が図形)。
// ちょうど 2.0 は明るい側。色は common/encode_color.slang が載せる。
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

    let corners = luma_at(tex, int2(0, 0))
        + luma_at(tex, int2(dims.x - 1, 0))
        + luma_at(tex, int2(0, dims.y - 1))
        + luma_at(tex, int2(dims.x - 1, dims.y - 1));

    let y = luma_at(tex, coord);
    let a = corners < 2.0 ? y : 1.0 - y;

    res.outputTex[coord] = float4(0.0, 0.0, 0.0, clamp(a, 0.0, 1.0));
}
