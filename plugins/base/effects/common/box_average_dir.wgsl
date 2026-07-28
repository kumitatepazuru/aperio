enable wgpu_binding_array;

#import aperio::blur::{plain_box_sum}

struct BoxAverageDirParams {
    radius: i32,
    // サンプルを進める方向(1ステップあたりの画素オフセット)。斜め方向を渡すと
    // 対角線ぶん(実距離で√2倍)サンプル間隔が伸びる(exedit-inspect glow README §5.2)。
    step_x: i32,
    step_y: i32,
    out_width: i32,
    out_height: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: BoxAverageDirParams;

// 方向ベクトルに沿った素のボックス平均(rgba全チャンネル、alpha非加重)。範囲外は
// ゼロ埋めのまま固定幅 (2*radius+1) で割る(=境界で自然にフェードする、実機の
// 「カーネル幅で割る素のボックス平均」と同じ挙動)。グローの`通常`形状の垂直パス・
// 仕上げ`ぼかし`(README §6)と、クロマキーの`境界補正`の垂直パスが共通で使う。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= params.out_width || coord.y >= params.out_height) {
        return;
    }

    let step = vec2<i32>(params.step_x, params.step_y);
    let radius = params.radius;

    let sum = plain_box_sum(tex, coord, step, radius);

    let divisor = f32(2 * radius + 1);
    textureStore(outputTex, coord, sum / divisor);
}
