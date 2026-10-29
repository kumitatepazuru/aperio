import aperio_bindings;

struct AccumulateSaturatingParams {
    float offset;
    int is_first;
};

struct InputTextures { Texture2D<float4> tex0; Texture2D<float4> tex1; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<AccumulateSaturatingParams>> res;

// 拡散速度0用の輝度・色差**結合**飽和アキュムレータ(実機 sub_100540a0/sub_100544a0、
// README手順4「蓄積は単純加算ではない」)。輝度が飽和すると色差が退色していく
// (白へ薄まる)挙動があり、色差の退色量が輝度の合計に依存するため輝度と色差を
// 別々のアキュムレータには分けられない。1枚 {r=Σcr, g=Σcb, a=Σy} で持ち回る。
//
// inputs.tex0: このパスでぼかし済みのチェーン(combined_init通過。r/g=色差(offset込)、
//              b=輝度量。拡散速度0なのでカーブは掛かっていない)
// inputs.tex1: これまでの蓄積 {r=Σcr, g=Σcb, a=Σy}(is_first!=0のときは読まない)
//
// 実機の整数表記(0x2000=満輝度の2倍, 0x4000=4倍, >>13=/(2*4096))を正規化空間に
// 落とすと 2.0 / 4.0 / /2.0 になる。t<0(s>4.0)のとき max(0,t)=0 で色差が全捨てに
// なるのも実機の acc.cb=acc.cr=0 分岐と一致する。
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

    let chain = inputs.tex0.Load(int3(coord, 0));
    let pass_cr = chain.r - res.params[0].offset;
    let pass_cb = chain.g - res.params[0].offset;
    let pass_y = chain.b;

    var acc_cr: float = 0.0;
    var acc_cb: float = 0.0;
    var acc_y: float = 0.0;
    if (res.params[0].is_first == 0) {
        let old = inputs.tex1.Load(int3(coord, 0));
        acc_cr = old.r;
        acc_cb = old.g;
        acc_y = old.a;
    }

    let s = acc_y + pass_y;
    if (s <= 2.0) {
        acc_y = s;
        acc_cr = acc_cr + pass_cr;
        acc_cb = acc_cb + pass_cb;
    } else {
        acc_y = 2.0;
        let t = max(0.0, (4.0 - s) / 2.0);
        acc_cr = (acc_cr + pass_cr) * t;
        acc_cb = (acc_cb + pass_cb) * t;
    }

    res.outputTex[coord] = float4(acc_cr, acc_cb, 0.0, acc_y);
}
