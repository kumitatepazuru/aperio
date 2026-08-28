enable wgpu_binding_array;

#import aperio::color::{bt601_luma}

struct MediaCompositeModeParams {
    // 0=色情報を上書き, 1=輝度をアルファ値として上書き, 2=輝度をアルファ値として乗算
    mode: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: MediaCompositeModeParams;

// exedit-inspect composite_video/composite_image README 共通の3合成モード。
// inputTex[0]=配置済みメディア(対象と同じキャンバスサイズ、矩形外・透明画素はalpha=0)、
// inputTex[1]=対象オブジェクト。matches both effects (`composite_video`, `composite_image`)。
// メディア側alpha=0の画素は「矩形の外」と「メディア自体の透明画素」を区別しないため、
// どちらも対象を透明にクロップする(README「矩形外はalpha=0にクロップされる」の簡略化)。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let media_tex = inputTex[0];
    let base_tex = inputTex[1];
    let dims = vec2<i32>(textureDimensions(base_tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let src = textureLoad(media_tex, coord, 0);
    let base = textureLoad(base_tex, coord, 0);

    var out: vec4<f32>;
    if (src.a <= 0.0) {
        out = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    } else if (params.mode == 0) {
        out = vec4<f32>(src.rgb, base.a);
    } else if (params.mode == 1) {
        out = vec4<f32>(base.rgb, clamp(bt601_luma(src.rgb), 0.0, 1.0));
    } else {
        out = vec4<f32>(base.rgb, clamp(bt601_luma(src.rgb) * base.a, 0.0, 1.0));
    }

    textureStore(outputTex, coord, out);
}
