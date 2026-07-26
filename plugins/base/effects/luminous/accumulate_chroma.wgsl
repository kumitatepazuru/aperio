struct AccumulateChromaParams {
    offset: f32,
    is_first: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: AccumulateChromaParams;

// 符号付きバイト127相当。yc48の2032 = 正規化 2032/4096 = 0.49609375
const CHROMA_LIMIT: f32 = 2032.0 / 4096.0;

// inputTex[0]: このパスでぼかし済みの色差チェーン(chroma_init.wgsl/combined_init.wgsl
//              のoffset込み、.r=Cr偏差、.g=Cb偏差)。輝度・色差の6パスを1本化した
//              経路(combined_init.wgsl)では.bに輝度量が入っているため、ここでは
//              .rgの2チャンネルだけを読み、.bには一切触れない。
// inputTex[1]: これまでの色差蓄積値([-1,1]にクランプ済み、同じくCr/Cbの2チャンネル)
//              (is_first!=0のときは読まない。呼び出し側もこの1パス目だけは
//              inputTex[1]を用意せず、テクスチャを1枚も無駄に生成しない)
//
// AviUtl版の元実装は色差(Cb/Cr)を「毎パス加算してすぐ[-128,127]にクランプ
// する」という粗い8bit実装のままアキュムレートしている。輝度側(curve.wgsl)
// のようななだらかな(exp/log空間での)救済は無く、単純加算+都度クランプ
// なので、彩度の高い光色×拡散速度>0では数パスで色差が頭打ちになり、それ
// 以降のパスは明るさ(輝度)だけが伸びて色が白っぽく薄まっていく
// (最終的な色への反映はreconstruct.wgslのBT.601逆変換で行う)。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let dims = vec2<i32>(textureDimensions(inputTex[0]));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let new_chain = textureLoad(inputTex[0], coord, 0).rg;
    let contribution = new_chain - vec2<f32>(params.offset);

    var old_accum = vec2<f32>(0.0);
    if (params.is_first == 0) {
        old_accum = textureLoad(inputTex[1], coord, 0).rg;
    }
    // 実機は色差を符号付きバイトへ落とし毎パス[-128,127]でクランプする(README手順5)。
    // (c+8)>>4 で yc48 の±2048が±128に対応するので上限127 = yc48 2032 = 正規化
    // 2032/4096。現状の±1.0は飽和が2倍遅く、純赤のcrが実機の約2倍残って色が
    // 黄色ではなく赤寄りに(#ffff79 が #ff9b6e に)ずれていた。
    let new_accum = clamp(old_accum + contribution, vec2<f32>(-CHROMA_LIMIT), vec2<f32>(CHROMA_LIMIT));

    textureStore(outputTex, coord, vec4<f32>(new_accum, 0.0, 1.0));
}
