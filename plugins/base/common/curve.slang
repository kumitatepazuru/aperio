import aperio_math;
import aperio_bindings;

struct CurveParams {
    float base;
    int inverse;
    // 変換対象のチャンネル(0=r, 1=g, 2=b, 3=a)。他のチャンネルは素通しする。
    int channel;
};

// 元wgslは texture_storage_2d<rgba32float, write> にハードコードされていたが、
// これはユーザーのテクスチャフォーマット選択(docs/plugin.md)を無視してしまう。
// 呼び出し側は全箇所ですでに min_output_format=Rgba32Float を渡しているため、
// 汎用のImageStorageTexture(+floor)方式に統一しても実際のバインド先フォーマットは
// 変わらない。
struct InputTextures { Texture2D<float4> tex0; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<CurveParams>> res;

// べき平均カーブの往復変換。ぼかし前に対象量(0~1程度の値)を
// pow(base, r*256)-1 で指数的に持ち上げ、ぼかし後に対数で戻す
// (ちょうど逆関数になっている)。baseが1より大きいほど、ぼかしカーネル内で
// 最も大きい値の寄与が単純平均より強く効くようになる(LogSumExp的な
// softmaxに近い挙動)。発光の拡散速度・ぼかしの光の強さで共通の式
// (どちらもREADMEで実機ヘルパー関数が同一と確認済み)。対象外のチャンネルは
// 素通しする。
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
    // 動的な添字での書き込み(s[params.channel] = ...)を避けるため、one-hot
    // マスクとdot積で動的な読み書きを行う(元wgslのHLSLトランスパイル制約回避策を
    // そのまま踏襲)。
    let mask = float4(
        res.params[0].channel == 0 ? 1.0 : 0.0,
        res.params[0].channel == 1 ? 1.0 : 0.0,
        res.params[0].channel == 2 ? 1.0 : 0.0,
        res.params[0].channel == 3 ? 1.0 : 0.0,
    );
    let r_in = dot(s, mask);
    var r: float = r_in;
    if (res.params[0].inverse == 0) {
        r = curve_forward(res.params[0].base, r);
    } else {
        r = curve_inverse(res.params[0].base, r);
    }
    let out = s + mask * (r - r_in);

    res.outputTex[coord] = out;
}
