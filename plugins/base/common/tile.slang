import aperio_bindings;

struct TileParams {
    // タイルの位相シフト(exedit-inspect composite_image README §5:
    // 「負の座標をx%dw/y%dhで畳んだ位置から始めて敷き詰める」)。
    int offset_x;
    int offset_y;
    int out_width;
    int out_height;
};

struct InputTextures { Texture2D<float4> tex0; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<TileParams>> res;

// パターン画像(inputs.tex0)を出力サイズいっぱいに単純タイルする(補間なし)。
// タイル原点は常に(0,0)。`シャドー`(exedit-inspect shadow README §6。影の箱の
// 左上が原点)と`縁取り`(exedit-inspect border README §6。キャンバスの左上が
// 原点)は原点の取り方が一致するため共通化できる。
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
    let pattern_dims = int2(dimsU, dimsV);
    let shifted = out_coord - int2(res.params[0].offset_x, res.params[0].offset_y);
    let in_coord = ((shifted % pattern_dims) + pattern_dims) % pattern_dims;

    res.outputTex[out_coord] = tex.Load(int3(in_coord, 0));
}
