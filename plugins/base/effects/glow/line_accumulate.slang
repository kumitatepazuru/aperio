import aperio_color;
import aperio_blur;
import aperio_bindings;

struct LineAccumulateParams {
    int radius;
    int step_x;
    int step_y;
    // gain = 強さ(UI 100%=1.0) * パス倍率 / 1638.4。ウィンドウ内の和(平均ではない)にこれを掛けて
    // 加算する。ウィンドウ幅が利得に直接効くのが実機の「カーネル幅で割らない蓄積」
    // (exedit-inspect glow README §4)。
    float gain;
    // 1: 既存の蓄積(inputs.tex1)を読まず0として扱う(最初の加算)。
    int is_first;
    int out_width;
    int out_height;
};

struct InputTextures { Texture2D<float4> tex0; Texture2D<float4> tex1; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<LineAccumulateParams>> res;

// inputs.tex0: 和を取る元テクスチャ(通常形状は垂直ボックス平均後、ストリーク系は
//              拡張済みの明部抽出結果を直接)
// inputs.tex1: これまでの蓄積(.rgb=発光量、.a未使用。is_first!=0のときは読まない)
//
// 方向ベクトルに沿ったボックス**和**(平均ではない)を gain 倍して蓄積へ加算し、
// 輝度(bt601_luma)が満輝度の2倍(2.0)を超えたら色を比率保存でリスケールする
// (README §4: 輝度8192で頭打ち、色差は比を保ったまま再スケールされ白飛びしない)。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let tex = inputs.tex0;
    let coord = int2(global_id.xy);

    if (coord.x >= res.params[0].out_width || coord.y >= res.params[0].out_height) {
        return;
    }

    let step = int2(res.params[0].step_x, res.params[0].step_y);
    let radius = res.params[0].radius;

    let sum_rgb = plain_box_sum(tex, coord, step, radius).rgb;

    var old_rgb: float3 = float3(0.0, 0.0, 0.0);
    if (res.params[0].is_first == 0) {
        old_rgb = inputs.tex1.Load(int3(coord, 0)).rgb;
    }

    var new_rgb = old_rgb + sum_rgb * res.params[0].gain;
    let luma = bt601_luma(new_rgb);
    if (luma > 2.0) {
        new_rgb = new_rgb * (2.0 / luma);
    }

    res.outputTex[coord] = float4(new_rgb, min(luma, 2.0));
}
