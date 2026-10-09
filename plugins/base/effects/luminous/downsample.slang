import aperio_bindings;

struct DownsampleParams {
    int factor;
    int out_width;
    int out_height;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<DownsampleParams>> res;

// 「高速化」ONの近似ぼかし用: 元画像を factor×factor の窓で単純平均して
// 1/factor に縮小する。以降の大半径ボックスぼかしをこの縮小画像上で行うことで
// タップ数を factor^2 分の1に抑える。RGBA一括で平均する(色差offset・輝度・
// a=1 はいずれも線形なので保存される)。窓が画像端をはみ出す部分は数に入れず、
// 実際に存在する画素だけの平均を取る(縮小前の box が範囲外を0扱いするのと同じ、
// 端に張り付いた偏りを避ける)。
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
    let in_dims = int2(dimsU, dimsV);
    let base = out_coord * res.params[0].factor;

    var acc: float4 = float4(0.0, 0.0, 0.0, 0.0);
    var count: float = 0.0;
    for (int dy = 0; dy < res.params[0].factor; dy++) {
        for (int dx = 0; dx < res.params[0].factor; dx++) {
            let p = base + int2(dx, dy);
            if (p.x < in_dims.x && p.y < in_dims.y) {
                acc += tex.Load(int3(p, 0));
                count += 1.0;
            }
        }
    }

    var result: float4 = float4(0.0, 0.0, 0.0, 0.0);
    if (count > 0.0) {
        result = acc / count;
    }
    res.outputTex[out_coord] = result;
}
