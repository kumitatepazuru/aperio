enable wgpu_binding_array;

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

// アルファだけを反転した(1-alpha)バッファを実体化する(exedit-inspect light README §3
// 「逆光オン: box_average(4096-alpha, r)」)。この後段の box_average_dir は範囲外を
// ゼロ埋めするので、先に反転バッファを作ってから通すことで「範囲外は元アルファ=0」
// という正しい境界セマンティクスになる(1-box_average(alpha)という近似では範囲外が
// 元アルファ=1相当になってしまい誤り)。rgbは使わないのでそのまま素通しする。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let s = textureLoad(tex, coord, 0);
    textureStore(outputTex, coord, vec4<f32>(s.rgb, 1.0 - s.a));
}
