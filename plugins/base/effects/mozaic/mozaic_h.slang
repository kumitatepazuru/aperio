import aperio_math;
import aperio_bindings;

struct MozaicHParams {
    int size;
    int center_x;
    int width;
    int height;
};

struct InputTextures { Texture2D<float4> tex0; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<MozaicHParams>> res;

[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let out_coord = int2(global_id.xy);
    let width = res.params[0].width;
    let height = res.params[0].height;

    if (out_coord.x >= width || out_coord.y >= height) {
        return;
    }

    let size = res.params[0].size;
    let tex = inputs.tex0;

    // 中心(center_x)を基準にsize幅で区切ったブロックのx範囲。
    // 画像端は割り切れず切り取られた状態のブロックになる。
    let dx = out_coord.x - res.params[0].center_x;
    let kx = floor_div(dx, size);
    let block_start_x = res.params[0].center_x + kx * size;
    let start_x = max(block_start_x, 0);
    let end_x = min(block_start_x + size, width);

    // 透明ピクセルとの合成で色が滲まないようプリマルチプライドアルファで平均化する
    var sum: float4 = float4(0.0, 0.0, 0.0, 0.0);
    for (int x = start_x; x < end_x; x++) {
        let s = tex.Load(int3(x, out_coord.y, 0));
        sum += float4(s.rgb * s.a, s.a);
    }

    let count = float(max(end_x - start_x, 1));
    res.outputTex[out_coord] = sum / count;
}
