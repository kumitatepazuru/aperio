import aperio_blur;
import aperio_bindings;

struct BoxBlurDirParams {
    int radius;
    // サンプルを進める方向(1ステップあたりの画素オフセット)。水平パスは(1,0)、
    // 垂直パスは(0,1)を渡す(旧box_blur_h.wgsl/box_blur_v.wgslの統合版)。
    int step_x;
    int step_y;
    int out_width;
    int out_height;
    // このパスの軸方向で、出力キャンバス内に入力を配置するシフト量。
    // キャンバスが伸びるパスでは半径分(中央に配置)、伸びないパスでは0。
    int offset;
    // 0: 端をクランプして常にサンプルする(luminousの近似ぼかし用、挙動不変)。
    // 1: 範囲外は寄与ゼロ(ぼかしフィルタの実機挙動、README手順4)。
    int border_mode;
    // 0: 固定カーネル幅で割る(端ほどアルファが薄まりフェードする、キャンバスが
    //    伸びる側で使う)。
    // 1: 有効サンプル数で割る(端で再正規化してオペークさを保つ、`サイズ固定`
    //    側で使う)。border_mode=0の時は全サンプルが常に有効なので無意味。
    int divisor_mode;
};

struct InputTextures { Texture2D<float4> tex0; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<BoxBlurDirParams>> res;

// 一様重みのボックス(矩形)ぼかし。blur/diffusion_light/luminous/boundary_blur共通。
// AviUtl版のぼかしフィルタは各1Dパスがそれ単体で「プリマルチプライして加重和
// →色はsum_a(重みの総和)で正規化→アルファだけ除数(モード次第)で割る」まで
// 完結させ、次のパスへは非プリマルチプライドで渡す(README手順4)。そのため
// このシェーダーは入力・出力ともストレートアルファの1パス完結型にしてある。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let tex = inputs.tex0;
    let out_coord = int2(global_id.xy);
    let radius = res.params[0].radius;

    if (out_coord.x >= res.params[0].out_width || out_coord.y >= res.params[0].out_height) {
        return;
    }

    let step = int2(res.params[0].step_x, res.params[0].step_y);
    let in_coord = out_coord - step * res.params[0].offset;

    let r = box_sum(tex, in_coord, step, radius, res.params[0].border_mode);

    let divisor = res.params[0].divisor_mode != 0 ? float(max(r.valid_count, 1)) : float(2 * radius + 1);

    var out_color: float3 = float3(0.0, 0.0, 0.0);
    if (r.sum_weight > 1e-6) {
        out_color = r.sum_rgb / r.sum_weight;
    }
    let out_alpha = r.sum_weight / divisor;

    res.outputTex[out_coord] = float4(out_color, out_alpha);
}
