@group(0) @binding(0) var input_texture: texture_2d<f32>; // RGBA float32
@group(0) @binding(1) var output_texture: texture_storage_2d<rgba8unorm, write>; // RGBA u8 だが実体はBGRA

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let texture_xy = textureDimensions(input_texture, 0);

    if (global_id.x >= texture_xy.x || global_id.y >= texture_xy.y) {
        return;
    }

    let pixel_index = global_id.y * texture_xy.x + global_id.x;

    // textureLoad は i32 座標を要求する
    let rgba_f32 = textureLoad(input_texture, vec2<i32>(global_id.xy), 0);
    let bgra = vec4<f32>(rgba_f32.b, rgba_f32.g, rgba_f32.r, rgba_f32.a); // BGRA -> RGBA

    textureStore(output_texture, global_id.xy, bgra);
}
