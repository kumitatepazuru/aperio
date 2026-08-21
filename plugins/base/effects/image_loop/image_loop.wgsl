enable wgpu_binding_array;

struct ImageLoopParams {
    tile_w: i32,
    tile_h: i32,
    offset_x: i32,
    offset_y: i32,
    out_width: i32,
    out_height: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params_array: ImageLoopParams;

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let out_coord = vec2<i32>(global_id.xy);
    let out_dims = vec2<i32>(params_array.out_width, params_array.out_height);
    if (out_coord.x >= out_dims.x || out_coord.y >= out_dims.y) {
        return;
    }

    let tile_dims = vec2<i32>(params_array.tile_w, params_array.tile_h);
    let shifted = (out_coord % tile_dims) + vec2<i32>(params_array.offset_x, params_array.offset_y);
    let src_coord = ((shifted % tile_dims) + tile_dims) % tile_dims;

    textureStore(outputTex, out_coord, textureLoad(inputTex[0], src_coord, 0));
}
