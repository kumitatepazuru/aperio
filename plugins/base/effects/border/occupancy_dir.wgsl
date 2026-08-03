enable wgpu_binding_array;

#import aperio::blur::{plain_box_sum}

struct BorderOccupancyDirParams {
    radius: i32,
    // サンプルを進める方向。垂直パスは(0,1)、水平パスは(1,0)を渡す
    // (common/box_average_dir.wgsl 等と同じ「1本のシェーダーを方向違いで2回使う」規約)。
    step_x: i32,
    step_y: i32,
    out_width: i32,
    out_height: i32,
    // 1024/((カーネル幅-1)*ぼかし*0.01+1) を0..1へ正規化した値(exedit-inspect
    // border README §2)。垂直パス・水平パスの両方に同じ値を掛ける(README §4)。
    gain: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: BorderOccupancyDirParams;

// 縁取りの被覆マップパス(exedit-inspect border README §4「割らないボックス」)。
// カーネル幅で割らないアルファの素の和にgainを掛けて1.0で飽和させる —— 平均では
// なく和なので、完全被覆の窓はgainの大きさに応じて1.0を大きく超えてから飽和側に
// 張り付く(=硬い輪郭のダイレーション)。alpha専用で、rgbは読まない
// (plain_box_sumはvec4を丸ごと足すが、.aだけ使うのでrgb成分の合計は捨てられる)。
// 出力もalphaだけを詰め、rgbは常に0(次段が.aしか読まないスクラッチプレーン)。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= params.out_width || coord.y >= params.out_height) {
        return;
    }

    let step = vec2<i32>(params.step_x, params.step_y);
    let sum = plain_box_sum(tex, coord, step, params.radius).a;
    let occupancy = min(1.0, sum * params.gain);

    textureStore(outputTex, coord, vec4<f32>(0.0, 0.0, 0.0, occupancy));
}
