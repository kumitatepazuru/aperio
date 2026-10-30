import aperio_math;
import aperio_bindings;

struct UpsampleParams {
    int factor;
    int out_width;
    int out_height;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<UpsampleParams>> res;

// 「高速化」ONの近似ぼかし用: downsample.slang で縮小しボックスぼかしをかけた
// 小画像を、元の解像度へバイリニア補間で戻す。downsample は小画像の画素 j を
// 元座標 j*factor + (factor-1)/2 の位置に対応づけるので、その逆写像
//   src = (out_coord - (factor-1)/2) / factor
// を小画像座標として4点バイリニア補間する(sampler不要、Load 4点)。
// 端は最近傍にクランプ(np.interp が両端でクランプするのと同じ)。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let out_coord = int2(global_id.xy);
    if (out_coord.x >= res.params[0].out_width || out_coord.y >= res.params[0].out_height) {
        return;
    }

    let tex = inputs.tex0;
    let f = float(res.params[0].factor);

    let src = (float2(out_coord) - float2((f - 1.0) * 0.5, (f - 1.0) * 0.5)) / f;
    let result = bilinear4_load(tex, src);

    res.outputTex[out_coord] = result;
}
