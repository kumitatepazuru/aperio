module aperio_bindings;

// 可変長の入力テクスチャ配列(実行時に決まる値でインデックスするシェーダーでのみ使う。
// リテラル定数でのインデックスはMesaドライバのバグを踏むため、固定長シェーダーは
// 代わりに各ファイル自身にインラインの非配列struct(tex0, tex1, ...)を定義すること)。
public struct InputTexturesVar {
    public Texture2D<float4> tex[];
};

// 出力ストレージテクスチャのみを持つリソースグループ(paramsを持たないシェーダー用)。
public struct EffectResourcesNoParams {
    [vk::image_format(APERIO_IMAGE_FORMAT)]
    public RWTexture2D<float4> outputTex;
};

// 出力ストレージテクスチャ + paramsバッファを持つリソースグループ。
public struct EffectResources<Params> {
    [vk::image_format(APERIO_IMAGE_FORMAT)]
    public RWTexture2D<float4> outputTex;
    public StructuredBuffer<Params> params;
};

// 出力ストレージテクスチャ + サンプラー + paramsバッファを持つリソースグループ。
// nagaのSPIR-Vフロントエンドは配列テクスチャに対する`Load`/`GetDimensions`単体には
// 対応していないため(`invalid image`)、可変長の入力テクスチャ配列を扱うシェーダーが
// テクセル単位の値を読みたい場合は、point/nearestサンプラーを介した`SampleLevel`
// (テクセル中心のUVを使えば`Load`と数値的に同一)を使う必要がある。
public struct EffectResourcesSampler<Params> {
    [vk::image_format(APERIO_IMAGE_FORMAT)]
    public RWTexture2D<float4> outputTex;
    public SamplerState samp;
    public StructuredBuffer<Params> params;
};
