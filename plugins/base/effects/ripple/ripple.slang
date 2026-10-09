import aperio_math;
import aperio_bindings;

static const float PI = 3.14159265358979;

struct RippleParams {
    float center_x;
    float center_y;
    float ring_spacing;
    float max_displacement;
    float wavefront;
    int ring_count;
    int ring_interval;
    int ramp_count;
    int out_width;
    int out_height;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<RippleParams>> res;

[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let out_coord = int2(global_id.xy);
    let out_dims = int2(res.params[0].out_width, res.params[0].out_height);
    if (out_coord.x >= out_dims.x || out_coord.y >= out_dims.y) {
        return;
    }

    let tex = inputs.tex0;
    let dx = float(out_coord.x) - (float(out_dims.x) * 0.5 + res.params[0].center_x);
    let dy = float(out_coord.y) - (float(out_dims.y) * 0.5 + res.params[0].center_y);
    let r = sqrt(dx * dx + dy * dy);

    if (r < 1e-4) {
        res.outputTex[out_coord] = tex.Load(int3(out_coord, 0));
        return;
    }

    let u = (res.params[0].wavefront - r) / (res.params[0].ring_spacing * 2.0);
    let ring_index_f = floor(u);
    let frac_u = u - ring_index_f;
    var mag = (cos(2.0 * PI * frac_u) - 1.0) * res.params[0].max_displacement / 2.0;

    let abs_idx = int(abs(ring_index_f));
    var scale: float = 1.0;
    if (res.params[0].ring_count > 0 && abs_idx >= res.params[0].ring_count) {
        scale = 0.0;
    }
    if (scale > 0.0 && res.params[0].ring_interval > 0 && (abs_idx % res.params[0].ring_interval) != 0) {
        scale = 0.0;
    }
    if (scale > 0.0 && res.params[0].ramp_count != 0) {
        if (res.params[0].ramp_count > 0) {
            scale = scale * clamp(float(abs_idx) / float(res.params[0].ramp_count), 0.0, 1.0);
        } else {
            scale = scale * clamp(1.0 - float(abs_idx) / float(-res.params[0].ramp_count), 0.0, 1.0);
        }
    }
    mag = mag * scale;

    if (mag == 0.0) {
        res.outputTex[out_coord] = tex.Load(int3(out_coord, 0));
        return;
    }

    let radial_unit = float2(dx, dy) / r;
    let sample_pos = float2(out_coord) + radial_unit * mag;
    res.outputTex[out_coord] = bilinear4_load_transparent(tex, sample_pos);
}
