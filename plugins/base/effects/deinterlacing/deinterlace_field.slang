import aperio_color;
import aperio_bindings;

struct DeinterlaceFieldParams {
    // 作り直す行の偶奇(0 = 偶数行 = `偶数解除`、1 = 奇数行 = `奇数解除`)。
    int field_parity;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<DeinterlaceFieldParams>> res;

// 近傍1画素を {cr, cb, y, a} で取る。端は最近傍にクランプする
// (exedit-inspect deinterlacing README §7: 行ループの前に 行-1←行0 / 行h←行h-1 を
// 複製してから3タップを掛けるのと同じ)。
float4 load_ycca(Texture2D<float4> tex, int x, int y, int2 dims) {
    let coord = int2(clamp(x, 0, dims.x - 1), clamp(y, 0, dims.y - 1));
    let s = tex.Load(int3(coord, 0));
    return float4(bt601_encode(s.rgb), s.a);
}

// exedit-inspect deinterlacing README §5: `奇数解除` / `偶数解除`。
// field_parity 側の行を捨てて、上下の行の6近傍からエッジ適応補間(ELA)で作り直す。
// 捨てる標本 cur を「どの近傍が一番うまく説明できるか」で方向を推定しているだけで、
// 動きは一切見ていない。
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

    // 残す側のフィールドは素通し。
    if ((coord.y & 1) != res.params[0].field_parity) {
        res.outputTex[coord] = tex.Load(int3(coord, 0));
        return;
    }

    let x = coord.x;
    let y = coord.y;
    let cur = load_ycca(tex, x, y, dims);
    let up = load_ycca(tex, x, y - 1, dims);
    let down = load_ycca(tex, x, y + 1, dims);
    let ul = load_ycca(tex, x - 1, y - 1, dims);
    let ur = load_ycca(tex, x + 1, y - 1, dims);
    let dl = load_ycca(tex, x - 1, y + 1, dims);
    let dr = load_ycca(tex, x + 1, y + 1, dims);

    // 素直な縦補間 avg を基準にして、cur を avg より上手く説明できない候補は avg で
    // 置き換える。比較も選択も成分ごとに独立なので、Y・Cb・Cr・アルファはそれぞれ
    // 別々に「どの方向から取るか」を決める(README §5)。アルファも普通の成分として
    // 同じ判定を通る ―― プリマルチプライは無い。
    let avg = (up + down) * 0.5;
    let d0 = abs(cur - avg);

    // 斜め4本は重み 1/8、上下2本は重み 1/4。合計 4/8 + 2/4 = 1(README §5)。
    let diag = select(abs(cur - ul) > d0, avg, ul)
             + select(abs(cur - dl) > d0, avg, dl)
             + select(abs(cur - ur) > d0, avg, ur)
             + select(abs(cur - dr) > d0, avg, dr);
    let vert = select(abs(cur - up) > d0, avg, up)
             + select(abs(cur - down) > d0, avg, down);
    let out = diag * 0.125 + vert * 0.25;

    res.outputTex[coord] = float4(bt601_decode(out.x, out.y, out.z), out.w);
}
