// 入力テクスチャが無いシェーダー: group0(入力用ParameterBlock)は宣言せず、
// resだけを規約通りset 1に固定する(set 0は空)。
struct EffectResources {
    [vk::image_format(APERIO_IMAGE_FORMAT)]
    RWTexture2D<float4> outputTex;
};
[vk::binding(0, 1)]
ParameterBlock<EffectResources> res;

[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    uint width, height;
    res.outputTex.GetDimensions(width, height);
    if (global_id.x >= width || global_id.y >= height) {
        return;
    }
    res.outputTex[int2(global_id.xy)] = float4(0.0, 0.0, 0.0, 1.0);
}
