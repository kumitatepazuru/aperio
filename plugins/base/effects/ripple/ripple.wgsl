enable wgpu_binding_array;

#import aperio::math::{bilinear4_load_transparent}

const PI: f32 = 3.14159265358979;

struct RippleParams {
    center_x: f32,
    center_y: f32,
    ring_spacing: f32,
    max_displacement: f32,
    wavefront: f32,
    ring_count: i32,
    ring_interval: i32,
    ramp_count: i32,
    out_width: i32,
    out_height: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params_array: RippleParams;

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let out_coord = vec2<i32>(global_id.xy);
    let out_dims = vec2<i32>(params_array.out_width, params_array.out_height);
    if (out_coord.x >= out_dims.x || out_coord.y >= out_dims.y) {
        return;
    }

    let tex = inputTex[0];
    let dx = f32(out_coord.x) - (f32(out_dims.x) * 0.5 + params_array.center_x);
    let dy = f32(out_coord.y) - (f32(out_dims.y) * 0.5 + params_array.center_y);
    let r = sqrt(dx * dx + dy * dy);

    if (r < 1e-4) {
        textureStore(outputTex, out_coord, textureLoad(tex, out_coord, 0));
        return;
    }

    let u = (params_array.wavefront - r) / (params_array.ring_spacing * 2.0);
    let ring_index_f = floor(u);
    let frac_u = u - ring_index_f;
    var mag = (cos(2.0 * PI * frac_u) - 1.0) * params_array.max_displacement / 2.0;

    let abs_idx = i32(abs(ring_index_f));
    var scale = 1.0;
    if (params_array.ring_count > 0 && abs_idx >= params_array.ring_count) {
        scale = 0.0;
    }
    if (scale > 0.0 && params_array.ring_interval > 0 && (abs_idx % params_array.ring_interval) != 0) {
        scale = 0.0;
    }
    if (scale > 0.0 && params_array.ramp_count != 0) {
        if (params_array.ramp_count > 0) {
            scale = scale * clamp(f32(abs_idx) / f32(params_array.ramp_count), 0.0, 1.0);
        } else {
            scale = scale * clamp(1.0 - f32(abs_idx) / f32(-params_array.ramp_count), 0.0, 1.0);
        }
    }
    mag = mag * scale;

    if (mag == 0.0) {
        textureStore(outputTex, out_coord, textureLoad(tex, out_coord, 0));
        return;
    }

    let radial_unit = vec2<f32>(dx, dy) / r;
    let sample_pos = vec2<f32>(out_coord) + radial_unit * mag;
    textureStore(outputTex, out_coord, bilinear4_load_transparent(tex, sample_pos));
}
