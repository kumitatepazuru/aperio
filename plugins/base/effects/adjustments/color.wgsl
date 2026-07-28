enable wgpu_binding_array;

#import aperio::color::{rgb_to_hsl, hsl_to_rgb}

struct ColorParams {
    brightness: f32,
    contrast: f32,
    hue: f32,
    luminance: f32,
    saturation: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;
@group(0) @binding(2) var linear_sampler: sampler;

@group(1) @binding(0) var<storage, read> params_array: ColorParams;

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let src = textureLoad(tex, coord, 0);
    var rgb = src.rgb;
    let alpha = src.a;

    // 明るさ: -1〜+1 の加算オフセット（100が中央）
    let brightness_offset = (params_array.brightness - 100.0) / 100.0;
    rgb = clamp(rgb + vec3<f32>(brightness_offset), vec3<f32>(0.0), vec3<f32>(1.0));

    // コントラスト: 0.5を中心としたスケール（100が等倍）
    let contrast_factor = params_array.contrast / 100.0;
    rgb = clamp((rgb - vec3<f32>(0.5)) * contrast_factor + vec3<f32>(0.5), vec3<f32>(0.0), vec3<f32>(1.0));

    // HSL空間で色相・彩度・輝度を調整
    var hsl = rgb_to_hsl(rgb);

    // 色相: 度数で回転（0が変化なし）
    let new_h = hsl.x + params_array.hue;
    hsl.x = new_h - 360.0 * floor(new_h / 360.0);

    // 彩度: 乗算（100が等倍）
    hsl.y = clamp(hsl.y * (params_array.saturation / 100.0), 0.0, 1.0);

    // 輝度: -1〜+1 の加算オフセット（100が中央）
    let luminance_offset = (params_array.luminance - 100.0) / 100.0;
    hsl.z = clamp(hsl.z + luminance_offset, 0.0, 1.0);

    rgb = hsl_to_rgb(hsl);

    textureStore(outputTex, coord, vec4<f32>(rgb, alpha));
}
