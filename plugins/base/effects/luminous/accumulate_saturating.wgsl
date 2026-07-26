struct AccumulateSaturatingParams {
    offset: f32,
    is_first: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;
@group(0) @binding(2) var linear_sampler: sampler;

@group(1) @binding(0) var<storage, read> params: AccumulateSaturatingParams;

// 拡散速度0用の輝度・色差**結合**飽和アキュムレータ(実機 sub_100540a0/sub_100544a0、
// README手順4「蓄積は単純加算ではない」)。輝度が飽和すると色差が退色していく
// (白へ薄まる)挙動があり、色差の退色量が輝度の合計に依存するため輝度と色差を
// 別々のアキュムレータには分けられない。1枚 {r=Σcr, g=Σcb, a=Σy} で持ち回る。
//
// inputTex[0]: このパスでぼかし済みのチェーン(combined_init通過。r/g=色差(offset込)、
//              b=輝度量。拡散速度0なのでカーブは掛かっていない)
// inputTex[1]: これまでの蓄積 {r=Σcr, g=Σcb, a=Σy}(is_first!=0のときは読まない)
//
// 実機の整数表記(0x2000=満輝度の2倍, 0x4000=4倍, >>13=/(2*4096))を正規化空間に
// 落とすと 2.0 / 4.0 / /2.0 になる。t<0(s>4.0)のとき max(0,t)=0 で色差が全捨てに
// なるのも実機の acc.cb=acc.cr=0 分岐と一致する。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let dims = vec2<i32>(textureDimensions(inputTex[0]));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let chain = textureLoad(inputTex[0], coord, 0);
    let pass_cr = chain.r - params.offset;
    let pass_cb = chain.g - params.offset;
    let pass_y = chain.b;

    var acc_cr = 0.0;
    var acc_cb = 0.0;
    var acc_y = 0.0;
    if (params.is_first == 0) {
        let old = textureLoad(inputTex[1], coord, 0);
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

    textureStore(outputTex, coord, vec4<f32>(acc_cr, acc_cb, 0.0, acc_y));
}
