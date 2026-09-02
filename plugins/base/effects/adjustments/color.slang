import aperio_color;
import aperio_bindings;

struct ColorParams {
    float brightness;
    float contrast;
    float hue;
    float luminance;
    float saturation;
};

struct InputTextures { Texture2D<float4> tex0; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<ColorParams>> res;

[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let tex = inputs.tex0;
    uint dimsU, dimsV;
    tex.GetDimensions(dimsU, dimsV);
    let dims = int2(dimsU, dimsV);
    let coord = int2(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let src = tex.Load(int3(coord, 0));
    var rgb: float3 = src.rgb;
    let alpha = src.a;

    // 明るさ: -1〜+1 の加算オフセット（100が中央）
    let brightness_offset = (res.params[0].brightness - 100.0) / 100.0;
    rgb = clamp(rgb + float3(brightness_offset), float3(0.0), float3(1.0));

    // コントラスト: 0.5を中心としたスケール（100が等倍）
    let contrast_factor = res.params[0].contrast / 100.0;
    rgb = clamp((rgb - float3(0.5)) * contrast_factor + float3(0.5), float3(0.0), float3(1.0));

    // HSL空間で色相・彩度・輝度を調整
    var hsl: float3 = rgb_to_hsl(rgb);

    // 色相: 度数で回転（0が変化なし）
    let new_h = hsl.x + res.params[0].hue;
    hsl.x = new_h - 360.0 * floor(new_h / 360.0);

    // 彩度: 乗算（100が等倍）
    hsl.y = clamp(hsl.y * (res.params[0].saturation / 100.0), 0.0, 1.0);

    // 輝度: -1〜+1 の加算オフセット（100が中央）
    let luminance_offset = (res.params[0].luminance - 100.0) / 100.0;
    hsl.z = clamp(hsl.z + luminance_offset, 0.0, 1.0);

    rgb = hsl_to_rgb(hsl);

    res.outputTex[coord] = float4(rgb, alpha);
}
