// 入力テクスチャが無いシェーダー: group0(入力用ParameterBlock)を宣言しないため、
// Slangはこの唯一のParameterBlockを自動的にset0へ割り当てる。
struct EffectResources {
    [vk::image_format(APERIO_IMAGE_FORMAT)]
    RWTexture2D<float4> outputTex;
};
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
