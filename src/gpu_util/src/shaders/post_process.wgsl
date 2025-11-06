@group(0) @binding(0) var input_texture: texture_2d<f32>; // RGBA float32
@group(0) @binding(1) var<storage, read_write> output_pixels: array<u32>; // RRGGBBAA u32 packed

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let texture_xy = textureDimensions(input_texture, 0);

    if (global_id.x >= texture_xy.x || global_id.y >= texture_xy.y) {
        return;
    }

    let pixel_index = global_id.y * texture_xy.x + global_id.x;

    // textureLoad は i32 座標を要求する
    let rgba_f32 = textureLoad(input_texture, vec2<i32>(global_id.xy), 0);

    let r = u32(clamp(rgba_f32.r, 0.0, 1.0) * 255.0);
    let g = u32(clamp(rgba_f32.g, 0.0, 1.0) * 255.0);
    let b = u32(clamp(rgba_f32.b, 0.0, 1.0) * 255.0);
    let a = u32(clamp(rgba_f32.a, 0.0, 1.0) * 255.0);

    // AABBGGRR の u32 にパック
    output_pixels[pixel_index] = (a << 24) | (b << 16) | (g << 8) | r;
}
