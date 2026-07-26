struct BoxBlurParams {
    radius: i32,
    out_width: i32,
    out_height: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params_array: BoxBlurParams;

// AviUtl版の発光フィルタが実際に使っているのは一様重みのボックス(矩形)ぼかしで
// あり、ガウシアン核ではないことが exedit.auf の命令列から確定している(README
// 手順4・9)。水平パス。入力・出力は同サイズで、窓が画像端をはみ出す分はエッジ
// 画素をクランプして拾う。プリマルチプライドのまま出力し、垂直パス側で
// un-premultiply する。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let in_dims = vec2<i32>(textureDimensions(tex));
    let out_coord = vec2<i32>(global_id.xy);
    let radius = params_array.radius;

    if (out_coord.x >= params_array.out_width || out_coord.y >= params_array.out_height) {
        return;
    }

    var color = vec4<f32>(0.0);
    for (var dx = -radius; dx <= radius; dx++) {
        let sc = clamp(out_coord + vec2<i32>(dx, 0), vec2<i32>(0, 0), in_dims - vec2<i32>(1, 1));
        let s = textureLoad(tex, sc, 0);
        color += vec4<f32>(max(s.rgb, vec3<f32>(0.0)) * s.a, s.a);
    }

    textureStore(outputTex, out_coord, color / f32(2 * radius + 1));
}
