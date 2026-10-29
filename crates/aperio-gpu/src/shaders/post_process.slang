// 最終的な f32 RGBA テクスチャをパックされた u32 (RRGGBBAA) のストレージバッファへ変換する。

[[vk::binding(0, 0)]]  // set 0: 入力
Texture2D<float4> input_texture;

[[vk::binding(0, 1)]]  // set 1: 出力
RWStructuredBuffer<uint> output_pixels;

[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    uint width, height;
    input_texture.GetDimensions(width, height);

    if (global_id.x >= width || global_id.y >= height) {
        return;
    }

    uint pixel_index = global_id.y * width + global_id.x;

    float4 rgba = input_texture.Load(int3(global_id.xy, 0));

    uint r = uint(clamp(rgba.r, 0.0, 1.0) * 255.0);
    uint g = uint(clamp(rgba.g, 0.0, 1.0) * 255.0);
    uint b = uint(clamp(rgba.b, 0.0, 1.0) * 255.0);
    uint a = uint(clamp(rgba.a, 0.0, 1.0) * 255.0);

    // AABBGGRR の u32 にパック
    output_pixels[pixel_index] = (a << 24) | (b << 16) | (g << 8) | r;
}
