enable f16; // https://docs.rs/wgpu/latest/wgpu/struct.Features.html#associatedconstant.SHADER_F16

@group(0) @binding(0) var input_texture: texture_2d<f32>; // RGBA float32
@group(0) @binding(1) var output_texture: texture_storage_2d<rgba16float, write>; // RGBA float16

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let texture_xy = textureDimensions(input_texture, 0);

    if (global_id.x >= texture_xy.x || global_id.y >= texture_xy.y) {
        return;
    }

    let rgba_f32 = textureLoad(input_texture, global_id.xy, 0);

    // ハードウェア側で rgba16float に変換される
    textureStore(output_texture, global_id.xy, rgba_f32);
}
