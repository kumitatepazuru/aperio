enable wgpu_binding_array;

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
        // タイル風: 左上辺にハイライト、右下辺にシャドウを付けるベベル。
        // 辺の判定はクリップ前のブロック範囲(画像外にはみ出しうる)から行う
        // ―― でないと画像端で切れたセルに本来出ないはずの縁取りが出てしまう。
        let dx = out_coord.x - params_array.center_x;
        let kx = floor_div(dx, size);
        let block_start_x = params_array.center_x + kx * size;

        let x_first = block_start_x;
        let x_last = block_start_x + size - 1;
        let y_first = block_start_y;
        let y_last = block_start_y + size - 1;

        // DARK(右下辺)がBRIGHT(左上辺)より優先される。1px幅のセルは
        // BRIGHTではなくDARKになる
        let is_dark = (out_coord.x == x_last || out_coord.y == y_last);
        let is_bright = !is_dark && (out_coord.x == x_first || out_coord.y == y_first);

        if (is_dark || is_bright) {
            let y = dot(out_color.rgb, vec3<f32>(0.299, 0.587, 0.114));
            let cr = (out_color.r - y) / 1.402000;
            let cb = (out_color.b - y) / 1.772000;

            var y2 = y;
            var cr2 = cr;
            var cb2 = cb;
            if (is_bright) {
                // ハイライトは輝度のみ1.25倍
                y2 = y + y * 0.25;
            } else {
                // シャドウは輝度・色差とも0.75倍
                y2 = y - y * 0.25;
                cr2 = cr - cr * 0.25;
                cb2 = cb - cb * 0.25;
            }

            // クランプなし(実機と同じく満輝度を超えた値もそのまま書き込む)
            let r = y2 + 1.402000 * cr2;
            let g = y2 - 0.344136 * cb2 - 0.714136 * cr2;
            let b = y2 + 1.772000 * cb2;
            out_color = vec4(r, g, b, out_color.a);
        }
    }

    textureStore(outputTex, out_coord, out_color);
}
