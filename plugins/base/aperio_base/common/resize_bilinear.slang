import aperio_math;
import aperio_bindings;

struct ResizeBilinearParams {
    int out_width;
    int out_height;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<ResizeBilinearParams>> res;

// bilinear4_loadによる汎用リサイズ(拡大・縮小どちらも同じ式でよい: srcは
// out_dimsに対するsrc_dimsの比率で決まるだけでスケール方向に依存しない)。
// テクセル中心基準の半画素シフトで写像する標準的なバイリニア。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let out_coord = int2(global_id.xy);

    if (out_coord.x >= res.params[0].out_width || out_coord.y >= res.params[0].out_height) {
        return;
    }

    let tex = inputs.tex0;
    uint dimsU, dimsV;
    tex.GetDimensions(dimsU, dimsV);
    let src_dims = int2(dimsU, dimsV);
    let scale = float2(src_dims) / float2(float(res.params[0].out_width), float(res.params[0].out_height));

    let src = (float2(out_coord) + float2(0.5, 0.5)) * scale - float2(0.5, 0.5);
    let color = bilinear4_load(tex, src);

    res.outputTex[out_coord] = color;
}
