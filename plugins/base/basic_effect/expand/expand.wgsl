enable wgpu_binding_array;

struct CanvasExtendParams {
    offset_x: i32,
    offset_y: i32,
    out_width: i32,
    out_height: i32,
    fill_mode: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;
@group(1) @binding(0) var<storage, read> params: CanvasExtendParams;

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let out_coord = vec2<i32>(global_id.xy);
    let out_dims = vec2<i32>(params.out_width, params.out_height);

    if (out_coord.x >= out_dims.x || out_coord.y >= out_dims.y) {
        return;
    }

    let tex = inputTex[0];
    let in_dims = vec2<i32>(textureDimensions(tex));
    let in_coord = out_coord - vec2<i32>(params.offset_x, params.offset_y);

    if (in_coord.x >= 0 && in_coord.x < in_dims.x && in_coord.y >= 0 && in_coord.y < in_dims.y) {
        textureStore(outputTex, out_coord, textureLoad(tex, in_coord, 0));
    } else if (params.fill_mode != 0) {
        let clamped = clamp(in_coord, vec2<i32>(0, 0), in_dims - vec2<i32>(1, 1));
        textureStore(outputTex, out_coord, textureLoad(tex, clamped, 0));
    } else {
        textureStore(outputTex, out_coord, vec4<f32>(0.0, 0.0, 0.0, 0.0));
    }
}
