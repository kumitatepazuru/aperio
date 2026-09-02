import aperio_blur;
import aperio_bindings;

struct BoxAverageDirParams {
    int radius;
    // サンプルを進める方向(1ステップあたりの画素オフセット)。斜め方向を渡すと
    // 対角線ぶん(実距離で√2倍)サンプル間隔が伸びる(exedit-inspect glow README §5.2)。
    int step_x;
    int step_y;
    int out_width;
    int out_height;
};

struct InputTextures { Texture2D<float4> tex0; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<BoxAverageDirParams>> res;

// 方向ベクトルに沿った素のボックス平均(rgba全チャンネル、alpha非加重)。範囲外は
// ゼロ埋めのまま固定幅 (2*radius+1) で割る(=境界で自然にフェードする、実機の
// 「カーネル幅で割る素のボックス平均」と同じ挙動)。グローの`通常`形状の垂直パス・
// 仕上げ`ぼかし`(README §6)と、クロマキーの`境界補正`の垂直パスが共通で使う。
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

    let sum = plain_box_sum(tex, coord, step, radius);

    let divisor = float(2 * radius + 1);
    res.outputTex[coord] = sum / divisor;
}
