enable wgpu_binding_array;

struct ResizeParams {
    out_width: i32,
    out_height: i32,
    no_interpolation: i32,
    _pad: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;
@group(1) @binding(0) var<storage, read> params: ResizeParams;

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let out_coord = vec2<i32>(global_id.xy);
    let out_dims = vec2<i32>(params.out_width, params.out_height);

    if (out_coord.x >= out_dims.x || out_coord.y >= out_dims.y) {
        return;
    }

    let tex = inputTex[0];
    let in_dims = vec2<i32>(textureDimensions(tex));

    if (params.no_interpolation != 0) {
        // 最近傍補間 (Nearest Neighbor)
        let in_x = clamp((out_coord.x * in_dims.x) / out_dims.x, 0, in_dims.x - 1);
        let in_y = clamp((out_coord.y * in_dims.y) / out_dims.y, 0, in_dims.y - 1);
        textureStore(outputTex, out_coord, textureLoad(tex, vec2<i32>(in_x, in_y), 0));
    } else {
        // バイリニア補間 (Bilinear Interpolation)
        let u = (f32(out_coord.x) + 0.5) * f32(in_dims.x) / f32(out_dims.x) - 0.5;
        let v = (f32(out_coord.y) + 0.5) * f32(in_dims.y) / f32(out_dims.y) - 0.5;

        let x0 = clamp(i32(floor(u)), 0, in_dims.x - 1);
        let y0 = clamp(i32(floor(v)), 0, in_dims.y - 1);
        let x1 = clamp(x0 + 1, 0, in_dims.x - 1);
        let y1 = clamp(y0 + 1, 0, in_dims.y - 1);

        let fx = clamp(u - floor(u), 0.0, 1.0);
        let fy = clamp(v - floor(v), 0.0, 1.0);

        let c00 = textureLoad(tex, vec2<i32>(x0, y0), 0);
        let c10 = textureLoad(tex, vec2<i32>(x1, y0), 0);
        let c01 = textureLoad(tex, vec2<i32>(x0, y1), 0);
        let c11 = textureLoad(tex, vec2<i32>(x1, y1), 0);

        let top = mix(c00, c10, fx);
        let bottom = mix(c01, c11, fx);
        let color = mix(top, bottom, fy);

        textureStore(outputTex, out_coord, color);
    }
}
