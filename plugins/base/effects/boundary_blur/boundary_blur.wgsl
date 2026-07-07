struct BoundaryBlurParams {
    radius: i32,
    aspect: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: BoundaryBlurParams;

const PI: f32 = 3.14159265358979323846;

fn fade_factor(dist: f32, R: f32) -> f32 {
    if (R <= 0.0) {
        return 1.0;
    }
    return sin(PI / 2.0 * clamp(dist / R, 0.0, 1.0));
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let W = dims.x;
    let H = dims.y;
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= W || coord.y >= H) {
        return;
    }

    let R = f32(params.radius);
    let A = f32(params.aspect);
    let Rx = R * min(1.0, 1.0 + A / 100.0);
    let Ry = R * min(1.0, 1.0 - A / 100.0);

    // テクスチャ境界モード: 画像の端からの距離でフェード
    let dx = f32(min(coord.x, W - 1 - coord.x));
    let dy = f32(min(coord.y, H - 1 - coord.y));
    let fx = fade_factor(dx, Rx);
    let fy = fade_factor(dy, Ry);

    let alpha_factor = fx * fy;
    let color = textureLoad(tex, coord, 0);
    textureStore(outputTex, coord, vec4<f32>(color.rgb, color.a * alpha_factor));
}
