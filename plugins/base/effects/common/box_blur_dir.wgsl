enable wgpu_binding_array;

#import aperio::blur::{box_sum}

struct BoxBlurDirParams {
    radius: i32,
    // サンプルを進める方向(1ステップあたりの画素オフセット)。水平パスは(1,0)、
    // 垂直パスは(0,1)を渡す(旧box_blur_h.wgsl/box_blur_v.wgslの統合版)。
    step_x: i32,
    step_y: i32,
    out_width: i32,
    out_height: i32,
    // このパスの軸方向で、出力キャンバス内に入力を配置するシフト量。
    // キャンバスが伸びるパスでは半径分(中央に配置)、伸びないパスでは0。
    offset: i32,
    // 0: 端をクランプして常にサンプルする(luminousの近似ぼかし用、挙動不変)。
    // 1: 範囲外は寄与ゼロ(ぼかしフィルタの実機挙動、README手順4)。
    border_mode: i32,
    // 0: 固定カーネル幅で割る(端ほどアルファが薄まりフェードする、キャンバスが
    //    伸びる側で使う)。
    // 1: 有効サンプル数で割る(端で再正規化してオペークさを保つ、`サイズ固定`
    //    側で使う)。border_mode=0の時は全サンプルが常に有効なので無意味。
    divisor_mode: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: BoxBlurDirParams;

// 一様重みのボックス(矩形)ぼかし。blur/diffusion_light/luminous/boundary_blur共通。
// AviUtl版のぼかしフィルタは各1Dパスがそれ単体で「プリマルチプライして加重和
// →色はsum_a(重みの総和)で正規化→アルファだけ除数(モード次第)で割る」まで
// 完結させ、次のパスへは非プリマルチプライドで渡す(README手順4)。そのため
// このシェーダーは入力・出力ともストレートアルファの1パス完結型にしてある。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let out_coord = vec2<i32>(global_id.xy);
    let radius = params.radius;

    if (out_coord.x >= params.out_width || out_coord.y >= params.out_height) {
        return;
    }

    let step = vec2<i32>(params.step_x, params.step_y);
    let in_coord = out_coord - step * params.offset;

    let r = box_sum(tex, in_coord, step, radius, params.border_mode);

    let divisor = select(f32(2 * radius + 1), f32(max(r.valid_count, 1)), params.divisor_mode != 0);

    var out_color = vec3<f32>(0.0);
    if (r.sum_weight > 1e-6) {
        out_color = r.sum_rgb / r.sum_weight;
    }
    let out_alpha = r.sum_weight / divisor;

    textureStore(outputTex, out_coord, vec4<f32>(out_color, out_alpha));
}
