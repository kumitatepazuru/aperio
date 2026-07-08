struct MozaicVParams {
    size: i32,
    center_x: i32,
    center_y: i32,
    width: i32,
    height: i32,
    tile_mode: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;
@group(0) @binding(2) var linear_sampler: sampler;

@group(1) @binding(0) var<storage, read> params_array: MozaicVParams;

fn floor_div(a: i32, b: i32) -> i32 {
    var q = a / b;
    let r = a % b;
    if (r != 0 && ((r < 0) != (b < 0))) {
        q = q - 1;
    }
    return q;
}

const BORDER_STRENGTH: f32 = 0.6;

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

    // 中心(center_y)を基準にしたブロックのy範囲
    let dy = out_coord.y - params_array.center_y;
    let ky = floor_div(dy, size);
    let block_start_y = params_array.center_y + ky * size;
    let start_y = max(block_start_y, 0);
    let end_y = min(block_start_y + size, height);

    // 水平パスの結果(プリマルチプライド)を縦方向にも平均し2Dブロック平均を完成させる
    var sum = vec4<f32>(0.0);
    for (var y = start_y; y < end_y; y++) {
        sum += textureLoad(tex, vec2<i32>(out_coord.x, y), 0);
    }

    let count = f32(max(end_y - start_y, 1));
    let premul = sum / count;

    var out_color: vec4<f32>;
    if (premul.a > 1e-6) {
        out_color = vec4(premul.rgb / premul.a, premul.a);
    } else {
        out_color = vec4(0.0);
    }

    if (params_array.tile_mode != 0) {
        // ブロック境界の1pxを暗くし、タイルの輪郭をはっきりさせる
        let dx = out_coord.x - params_array.center_x;
        let kx = floor_div(dx, size);
        let block_start_x = params_array.center_x + kx * size;
        let start_x = max(block_start_x, 0);
        let end_x = min(block_start_x + size, width);

        let on_x_edge = (out_coord.x == start_x || out_coord.x == end_x - 1);
        let on_y_edge = (out_coord.y == start_y || out_coord.y == end_y - 1);

        if (on_x_edge || on_y_edge) {
            out_color = vec4(out_color.rgb * BORDER_STRENGTH, out_color.a);
        }
    }

    textureStore(outputTex, out_coord, out_color);
}
