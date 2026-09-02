import aperio_blur;
import aperio_bindings;

struct BorderOccupancyDirParams {
    int radius;
    // サンプルを進める方向。垂直パスは(0,1)、水平パスは(1,0)を渡す
    // (common/box_average_dir.slang 等と同じ「1本のシェーダーを方向違いで2回使う」規約)。
    int step_x;
    int step_y;
    int out_width;
    int out_height;
    // 1024/((カーネル幅-1)*ぼかし*0.01+1) を0..1へ正規化した値(exedit-inspect
    // border README §2)。垂直パス・水平パスの両方に同じ値を掛ける(README §4)。
    float gain;
};

struct InputTextures { Texture2D<float4> tex0; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<BorderOccupancyDirParams>> res;

// 縁取りの被覆マップパス(exedit-inspect border README §4「割らないボックス」)。
// カーネル幅で割らないアルファの素の和にgainを掛けて1.0で飽和させる —— 平均では
// なく和なので、完全被覆の窓はgainの大きさに応じて1.0を大きく超えてから飽和側に
// 張り付く(=硬い輪郭のダイレーション)。alpha専用で、rgbは読まない
// (plain_box_sumはvec4を丸ごと足すが、.aだけ使うのでrgb成分の合計は捨てられる)。
// 出力もalphaだけを詰め、rgbは常に0(次段が.aしか読まないスクラッチプレーン)。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let tex = inputs.tex0;
    let coord = int2(global_id.xy);

    if (coord.x >= res.params[0].out_width || coord.y >= res.params[0].out_height) {
        return;
    }

    let step = int2(res.params[0].step_x, res.params[0].step_y);
    let sum = plain_box_sum(tex, coord, step, res.params[0].radius).a;
    let occupancy = min(1.0, sum * res.params[0].gain);

    res.outputTex[coord] = float4(0.0, 0.0, 0.0, occupancy);
}
