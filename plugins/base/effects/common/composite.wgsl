enable wgpu_binding_array;

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

// 2枚のストレートアルファ画像を標準的なプリマルチプライドsource-overで合成する
// 汎用ステップ。inputTex[0]が前面(src)、inputTex[1]が背面(dst)。
// `シャドー`(exedit-inspect shadow README §3。前面=キャンバスへ配置し直した
// オブジェクト、背面=影レイヤー)と`縁取り`(exedit-inspect border README §5。
// 前面=キャンバス中央に配置し直したオブジェクト、背面=縁レイヤー)が共通で使う。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let front_tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(front_tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let front = textureLoad(front_tex, coord, 0);
    let back = textureLoad(inputTex[1], coord, 0);

    let out_alpha = front.a + back.a * (1.0 - front.a);
    let premul = front.rgb * front.a + back.rgb * back.a * (1.0 - front.a);

    var out_rgb = vec3<f32>(0.0);
    if (out_alpha > 1e-6) {
        out_rgb = premul / out_alpha;
    }

    textureStore(outputTex, coord, vec4<f32>(out_rgb, clamp(out_alpha, 0.0, 1.0)));
}
