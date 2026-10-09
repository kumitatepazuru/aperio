import aperio_bindings;

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResourcesNoParams> res;

// 端は最近傍にクランプする(README §7: 行-1←行0 / 行h←行h-1 の複製と同じ。
// 端の行だけ重みが 3/4 : 1/4 に寄るのもここから出る)。
float4 load_row(Texture2D<float4> tex, int x, int y, int2 dims) {
    return tex.Load(int3(x, clamp(y, 0, dims.y - 1), 0));
}

// exedit-inspect deinterlacing README §6: `二重化`。全行に 1:2:1 の縦フィルタを掛ける。
// 1 - 2 + 1 = 0 なので縦のナイキスト(= 櫛そのもの)がきっちり消え、直流の利得は 1。
// フィールドで見れば自分のフィールドが 2/4、相手が 1/4 + 1/4 の 50:50 混合になる。
//
// 線形フィルタなので RGBA のまま掛けてよい ―― BT.601 変換は線形変換で、
// `worker_field` のような非線形の判定が無いため YCbCr で掛けた結果と一致する。
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

    let up = load_row(tex, coord.x, coord.y - 1, dims);
    let mid = tex.Load(int3(coord, 0));
    let down = load_row(tex, coord.x, coord.y + 1, dims);

    res.outputTex[coord] = (up + mid * 2.0 + down) * 0.25;
}
