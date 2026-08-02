enable wgpu_binding_array;

struct EncodeColorParams {
    // 濃さ(strength_raw / 1000.0)。0..1
    density: f32,
    color_r: f32,
    color_g: f32,
    color_b: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: EncodeColorParams;

// 影色版のエンコード(exedit-inspect shadow README §5)。inputTex[0]は4パスの
// ボックス平均を通したアルファ(.aだけ使う、正規化済みの [0,1] 平均)。
// 影色はそのままY/Cb/Crへ入り(アンプリマルチプライ無し)、被覆率だけを
// アルファに載せる — 濃さは avg に掛けるだけでクランプ以外の処理は不要。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let mask_tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(mask_tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let avg = textureLoad(mask_tex, coord, 0).a;
    let alpha = clamp(avg * params.density, 0.0, 1.0);

    textureStore(outputTex, coord, vec4<f32>(params.color_r, params.color_g, params.color_b, alpha));
}
