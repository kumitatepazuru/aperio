import aperio_color;
import aperio_bindings;

struct InputTextures { Texture2D<float4> tex0; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResourcesNoParams> res;

// RGB(ストレートアルファ)をBT.601の輝度Y・色差Cr/Cbへ変換し、
// {r=Cr偏差, g=Cb偏差, b=Y, a=alpha} に詰め替える。alphaは素通し。
// common/box_blur_dir.slang はチャンネルを区別せず
// アルファ加重平均するだけなので、このYCbCr表現のままボックスぼかしを
// 通してもRGBのまま通すのと同じ結果になる。
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

    let s = tex.Load(int3(coord, 0));
    let ycc = bt601_encode(s.rgb);

    res.outputTex[coord] = float4(ycc.x, ycc.y, ycc.z, s.a);
}
