import aperio_math;
import aperio_color;
import aperio_bindings;

struct MozaicVParams {
    int size;
    int center_x;
    int center_y;
    int width;
    int height;
    int tile_mode;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<MozaicVParams>> res;

[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let out_coord = int2(global_id.xy);
    let width = res.params[0].width;
    let height = res.params[0].height;

    if (out_coord.x >= width || out_coord.y >= height) {
        return;
    }

    let size = res.params[0].size;
    let tex = inputs.tex0;

    // 中心(center_y)を基準にしたブロックのy範囲
    let dy = out_coord.y - res.params[0].center_y;
    let ky = floor_div(dy, size);
    let block_start_y = res.params[0].center_y + ky * size;
    let start_y = max(block_start_y, 0);
    let end_y = min(block_start_y + size, height);

    // 水平パスの結果(プリマルチプライド)を縦方向にも平均し2Dブロック平均を完成させる
    var sum: float4 = float4(0.0, 0.0, 0.0, 0.0);
    for (int y = start_y; y < end_y; y++) {
        sum += tex.Load(int3(out_coord.x, y, 0));
    }

    let count = float(max(end_y - start_y, 1));
    let premul = sum / count;

    var out_color: float4;
    if (premul.a > 1e-6) {
        out_color = float4(premul.rgb / premul.a, premul.a);
    } else {
        out_color = float4(0.0, 0.0, 0.0, 0.0);
    }

    if (res.params[0].tile_mode != 0) {
        // タイル風: 左上辺にハイライト、右下辺にシャドウを付けるベベル。
        // 辺の判定はクリップ前のブロック範囲(画像外にはみ出しうる)から行う
        // ―― でないと画像端で切れたセルに本来出ないはずの縁取りが出てしまう。
        let dx = out_coord.x - res.params[0].center_x;
        let kx = floor_div(dx, size);
        let block_start_x = res.params[0].center_x + kx * size;

        let x_first = block_start_x;
        let x_last = block_start_x + size - 1;
        let y_first = block_start_y;
        let y_last = block_start_y + size - 1;

        // DARK(右下辺)がBRIGHT(左上辺)より優先される。1px幅のセルは
        // BRIGHTではなくDARKになる
        let is_dark = (out_coord.x == x_last || out_coord.y == y_last);
        let is_bright = !is_dark && (out_coord.x == x_first || out_coord.y == y_first);

        if (is_dark || is_bright) {
            let ycc = bt601_encode(out_color.rgb);
            let y = ycc.z;
            let cr = ycc.x;
            let cb = ycc.y;

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
            out_color = float4(bt601_decode(cr2, cb2, y2), out_color.a);
        }
    }

    res.outputTex[out_coord] = out_color;
}
