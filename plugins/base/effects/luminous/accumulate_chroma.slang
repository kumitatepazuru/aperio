import aperio_bindings;

struct AccumulateChromaParams {
    float offset;
    int is_first;
};

struct InputTextures { Texture2D<float4> tex0; Texture2D<float4> tex1; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<AccumulateChromaParams>> res;

// 符号付きバイト127相当。yc48の2032 = 正規化 2032/4096 = 0.49609375
static const float CHROMA_LIMIT = 2032.0 / 4096.0;

// inputs.tex0: このパスでぼかし済みの色差チェーン(chroma_init.slang/combined_init.slang
//              のoffset込み、.r=Cr偏差、.g=Cb偏差)。輝度・色差の6パスを1本化した
//              経路(combined_init.slang)では.bに輝度量が入っているため、ここでは
//              .rgの2チャンネルだけを読み、.bには一切触れない。
// inputs.tex1: これまでの色差蓄積値([-1,1]にクランプ済み、同じくCr/Cbの2チャンネル)
//              (is_first!=0のときは読まない。呼び出し側もこの1パス目だけは
//              inputs.tex1を用意せず、テクスチャを1枚も無駄に生成しない)
//
// AviUtl版の元実装は色差(Cb/Cr)を「毎パス加算してすぐ[-128,127]にクランプ
// する」という粗い8bit実装のままアキュムレートしている。輝度側(curve.slang)
// のようななだらかな(exp/log空間での)救済は無く、単純加算+都度クランプ
// なので、彩度の高い光色×拡散速度>0では数パスで色差が頭打ちになり、それ
// 以降のパスは明るさ(輝度)だけが伸びて色が白っぽく薄まっていく
// (最終的な色への反映はreconstruct.slangのBT.601逆変換で行う)。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    uint dimsU, dimsV;
    inputs.tex0.GetDimensions(dimsU, dimsV);
    let dims = int2(dimsU, dimsV);
    let coord = int2(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let new_chain = inputs.tex0.Load(int3(coord, 0)).rg;
    let contribution = new_chain - float2(res.params[0].offset, res.params[0].offset);

    var old_accum: float2 = float2(0.0, 0.0);
    if (res.params[0].is_first == 0) {
        old_accum = inputs.tex1.Load(int3(coord, 0)).rg;
    }
    // 実機は色差を符号付きバイトへ落とし毎パス[-128,127]でクランプする(README手順5)。
    // (c+8)>>4 で yc48 の±2048が±128に対応するので上限127 = yc48 2032 = 正規化
    // 2032/4096。現状の±1.0は飽和が2倍遅く、純赤のcrが実機の約2倍残って色が
    // 黄色ではなく赤寄りに(#ffff79 が #ff9b6e に)ずれていた。
    let new_accum = clamp(old_accum + contribution, float2(-CHROMA_LIMIT, -CHROMA_LIMIT), float2(CHROMA_LIMIT, CHROMA_LIMIT));

    res.outputTex[coord] = float4(new_accum, 0.0, 1.0);
}
