enable wgpu_binding_array;

#import aperio::math::{floor_div}

struct MozaicHParams {
    size: i32,
    center_x: i32,
    width: i32,
    height: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;
@group(0) @binding(2) var linear_sampler: sampler;

@group(1) @binding(0) var<storage, read> params_array: MozaicHParams;

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let out_coord = vec2<i32>(global_id.xy);
    let width = params_array.width;
    let height = params_array.height;

    if (out_coord.x >= width || out_coord.y >= height) {
        return;
    }

    let size = params_array.size;
    let tex = inputTex[0];

    // 中心(center_x)を基準にsize幅で区切ったブロックのx範囲。
    // 画像端は割り切れず切り取られた状態のブロックになる。
    let dx = out_coord.x - params_array.center_x;
    let kx = floor_div(dx, size);
    let block_start_x = params_array.center_x + kx * size;
    let start_x = max(block_start_x, 0);
    let end_x = min(block_start_x + size, width);

    // 透明ピクセルとの合成で色が滲まないようプリマルチプライドアルファで平均化する
    var sum = vec4<f32>(0.0);
    for (var x = start_x; x < end_x; x++) {
        let s = textureLoad(tex, vec2<i32>(x, out_coord.y), 0);
        sum += vec4(s.rgb * s.a, s.a);
    }

    let count = f32(max(end_x - start_x, 1));
    textureStore(outputTex, out_coord, sum / count);
}
