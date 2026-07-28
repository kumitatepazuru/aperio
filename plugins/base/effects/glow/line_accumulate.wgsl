enable wgpu_binding_array;

#import aperio::color::{bt601_luma}
#import aperio::blur::{plain_box_sum}

struct LineAccumulateParams {
    radius: i32,
    step_x: i32,
    step_y: i32,
    // gain = strength_raw / 16384。ウィンドウ内の和(平均ではない)にこれを掛けて
    // 加算する。ウィンドウ幅が利得に直接効くのが実機の「カーネル幅で割らない蓄積」
    // (exedit-inspect glow README §4)。
    gain: f32,
    // 1: 既存の蓄積(inputTex[1])を読まず0として扱う(最初の加算)。
    is_first: i32,
    out_width: i32,
    out_height: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: LineAccumulateParams;

// inputTex[0]: 和を取る元テクスチャ(通常形状は垂直ボックス平均後、ストリーク系は
//              拡張済みの明部抽出結果を直接)
// inputTex[1]: これまでの蓄積(.rgb=発光量、.a未使用。is_first!=0のときは読まない)
//
// 方向ベクトルに沿ったボックス**和**(平均ではない)を gain 倍して蓄積へ加算し、
// 輝度(bt601_luma)が満輝度の2倍(2.0)を超えたら色を比率保存でリスケールする
// (README §4: 輝度8192で頭打ち、色差は比を保ったまま再スケールされ白飛びしない)。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= params.out_width || coord.y >= params.out_height) {
        return;
    }

    let step = vec2<i32>(params.step_x, params.step_y);
    let radius = params.radius;

    let sum_rgb = plain_box_sum(tex, coord, step, radius).rgb;

    var old_rgb = vec3<f32>(0.0);
    if (params.is_first == 0) {
        old_rgb = textureLoad(inputTex[1], coord, 0).rgb;
    }

    var new_rgb = old_rgb + sum_rgb * params.gain;
    let luma = bt601_luma(new_rgb);
    if (luma > 2.0) {
        new_rgb = new_rgb * (2.0 / luma);
    }

    textureStore(outputTex, coord, vec4<f32>(new_rgb, min(luma, 2.0)));
}
