import aperio_bindings;

struct InputTextures { Texture2D<float4> tex0; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResourcesNoParams> res;

// アルファだけを反転した(1-alpha)バッファを実体化する(exedit-inspect light README §3
// 「逆光オン: box_average(4096-alpha, r)」)。この後段の box_average_dir は範囲外を
// ゼロ埋めするので、先に反転バッファを作ってから通すことで「範囲外は元アルファ=0」
// という正しい境界セマンティクスになる(1-box_average(alpha)という近似では範囲外が
// 元アルファ=1相当になってしまい誤り)。rgbは使わないのでそのまま素通しする。
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
    res.outputTex[coord] = float4(s.rgb, 1.0 - s.a);
}
