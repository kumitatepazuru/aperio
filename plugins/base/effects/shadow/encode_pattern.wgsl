enable wgpu_binding_array;

struct EncodePatternParams {
    // 濃さ(strength_raw / 1000.0)。0..1
    density: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: EncodePatternParams;

// パターン画像版のエンコード(exedit-inspect shadow README §5)。inputTex[0]は
// 4パスのボックス平均を通したアルファ(.aだけ使う)、inputTex[1]はタイル済み
// パターン画像(tile.wgsl の出力、ストレートRGBA)。
// 色パラメータは完全に無視し、パターンの色をそのまま使う。アルファ同士が
// 掛かるだけなので濃さとパターンの透明度は交換可能(README §5)。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let mask_tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(mask_tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let avg = textureLoad(mask_tex, coord, 0).a;
    let pattern = textureLoad(inputTex[1], coord, 0);
    let alpha = clamp(avg * pattern.a * params.density, 0.0, 1.0);

    textureStore(outputTex, coord, vec4<f32>(pattern.rgb, alpha));
}
