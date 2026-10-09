import aperio_color;
import aperio_bindings;

struct ConvexEdgeParams {
    int steps;
    // 単位方向ベクトル(exedit-inspect convex_edge README §3)。
    float dir_x;
    float dir_y;
    float height;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<ConvexEdgeParams>> res;

// exedit-inspect convex_edge README §4: 点対称な有限差分。ステップkごとの
// オフセットをfloor(k*dir)で整数化し、その符号反転を後方サンプルにそのまま使う
// (後方だけ別途floorし直さない)ことで点対称なサンプル対を保つ。
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

    let src = tex.Load(int3(coord, 0));
    let ycc = bt601_encode(src.rgb);
    let cr = ycc.x;
    let cb = ycc.y;
    let y = ycc.z;

    var acc: float2 = float2(0.0, 0.0);
    var sum: float = 0.0;
    let dir = float2(res.params[0].dir_x, res.params[0].dir_y);
    for (int k = 1; k <= res.params[0].steps; k++) {
        acc += dir;
        let o = int2(floor(acc));

        // README §4(c): 前後それぞれ独立に範囲判定する(枠外は寄与ゼロ)。
        let fwd = coord + o;
        if (fwd.x >= 0 && fwd.x < dims.x && fwd.y >= 0 && fwd.y < dims.y) {
            sum += tex.Load(int3(fwd, 0)).a;
        }
        let back = coord - o;
        if (back.x >= 0 && back.x < dims.x && back.y >= 0 && back.y < dims.y) {
            sum -= tex.Load(int3(back, 0)).a;
        }
    }

    // README §5: scale = 高さ_raw/(200*steps)。高さ(表示値) = 高さ_raw/100 を
    // 代入すると 200 が 2 に落ちる正規化空間での等価式。
    let scale = res.params[0].height / (2.0 * float(res.params[0].steps));
    let d = sum * scale;

    var out_y: float;
    var out_cb = cb;
    var out_cr = cr;
    if (d >= 0.0 || y <= 0.0) {
        // 明側: 加算のみ、クランプ無し(README §5)。
        out_y = y + d;
    } else {
        // 暗側: 下限0の1箇所だけクランプし、Y/Cb/Crを同じ係数で縮める(README §5)。
        let ny = max(0.0, y + d);
        let k = ny / y;
        out_y = ny;
        out_cb = cb * k;
        out_cr = cr * k;
    }

    let rgb = bt601_decode(out_cr, out_cb, out_y);
    res.outputTex[coord] = float4(rgb, src.a);
}
