import aperio_bindings;

struct SelectParams {
    int index;
};

ParameterBlock<InputTexturesVar> inputs;
ParameterBlock<EffectResourcesSampler<SelectParams>> res;

// inputs.tex[params.index] をそのままoutputTexへコピーするだけの素通しシェーダー。
// パイプラインのstateは1ステップごとに1枚に潰れるため、複数バッファを跨いで
// 保持したい場合(前段の結果を次段が読みつつ、同じ入力を後続の呼び出しでも
// 使い回したい場合)にparallelブランチの中でこれを使って明示的に複製・選別する。
// glow/luminous共通。
//
// nagaのSPIR-Vフロントエンドは配列テクスチャに対する`Load`が(リテラル/動的インデックス
// を問わず)`invalid image`になるため、`res.samp`(point/nearestサンプラー)経由の
// `SampleLevel`を使う。テクセル中心のUVを使えば結果は`Load`と数値的に同一になる。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let tex = inputs.tex[res.params[0].index];
    uint dimsU, dimsV;
    tex.GetDimensions(dimsU, dimsV);
    let dims = int2(dimsU, dimsV);
    let coord = int2(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let uv = (float2(coord) + float2(0.5, 0.5)) / float2(dims);
    res.outputTex[coord] = tex.SampleLevel(res.samp, uv, 0.0);
}
