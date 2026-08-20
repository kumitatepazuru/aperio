enable wgpu_binding_array;

struct Rotation90Params {
    mode: i32,
    out_width: i32,
    out_height: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;
@group(1) @binding(0) var<storage, read> params: Rotation90Params;

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let out_coord = vec2<i32>(global_id.xy);
    let out_dims = vec2<i32>(params.out_width, params.out_height);

    if (out_coord.x >= out_dims.x || out_coord.y >= out_dims.y) {
        return;
    }

    let tex = inputTex[0];
    let in_dims = vec2<i32>(textureDimensions(tex));

    var in_coord: vec2<i32>;
    if (params.mode == 1) {
        // 90° 時計回り
        in_coord = vec2<i32>(out_coord.y, in_dims.y - 1 - out_coord.x);
    } else if (params.mode == 2) {
        // 180°
        in_coord = vec2<i32>(in_dims.x - 1 - out_coord.x, in_dims.y - 1 - out_coord.y);
    } else if (params.mode == 3) {
        // 270° 時計回り (90° 反時計回り)
        in_coord = vec2<i32>(in_dims.x - 1 - out_coord.y, out_coord.x);
    } else {
        in_coord = out_coord;
    }

    textureStore(outputTex, out_coord, textureLoad(tex, in_coord, 0));
}
