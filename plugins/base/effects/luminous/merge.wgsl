@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

// inputTex[0]: 元画像を拡張キャンバスへ配置したもの(expand.wgsl通過済み、
//              非プリマルチプライド)
// inputTex[1]: 抽出・ぼかし済みの発光バッファ(rgbは光色固定、alphaが
//              強さ。非プリマルチプライド)
//
// 両者をプリマルチプライド形式に変換してから単純加算し(元のAviUtl版と
// 同じ非負バッファの加算合成)、最後に出力alphaで割って非プリマルチプライド
// に戻す。こうしないと、元画像が透明な領域(alpha=0)に発光がにじみ出た
// ときに光色が薄まって出力されてしまう。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let base_tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(base_tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let base = textureLoad(base_tex, coord, 0);
    let glow = textureLoad(inputTex[1], coord, 0);

    let base_premul = base.rgb * base.a;
    let glow_premul = glow.rgb * glow.a;
    // 実機の合成(sub_10053890、README手順6)はアルファを**加算**して4096で
    // クランプする。通常の a+b-a*b ではない(base.aが0か1の所では同じだが、
    // アンチエイリアス境界など半透明領域で差が出る)。
    let out_alpha = clamp(base.a + glow.a, 0.0, 1.0);

    var out_rgb = vec3<f32>(0.0);
    if (out_alpha > 1e-6) {
        out_rgb = clamp((base_premul + glow_premul) / out_alpha, vec3<f32>(0.0), vec3<f32>(1.0));
    }

    textureStore(outputTex, coord, vec4<f32>(out_rgb, out_alpha));
}
