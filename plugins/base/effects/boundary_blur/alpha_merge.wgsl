struct AlphaMergeParams {
    radius: i32,
    aspect: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: AlphaMergeParams;

const PI: f32 = 3.14159265358979323846;

fn fade_factor(dist: f32, R: f32) -> f32 {
    if (R <= 0.0) {
        return 1.0;
    }
    return sin(PI / 2.0 * clamp(dist / R, 0.0, 1.0));
}

// inputTex[0]: 元のカラー画像 (並列パイプラインでの素通しブランチ)
// inputTex[1]: xy = 島の中心座標(極大点), z = 不透明境界までの距離D
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let color_tex = inputTex[0];
    let data_tex = inputTex[1];
    let dims = vec2<i32>(textureDimensions(color_tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let color = textureLoad(color_tex, coord, 0);
    let data = textureLoad(data_tex, coord, 0);
    let peak = data.xy;
    let d = data.z;

    let R = f32(params.radius);
    let A = f32(params.aspect);
    let Rx = R * min(1.0, 1.0 + A / 100.0);
    let Ry = R * min(1.0, 1.0 - A / 100.0);

    // 島の中心から見た方向によって、縦横比(aspect)を楕円として反映する。
    var dir = vec2<f32>(coord) - peak;
    let len = length(dir);

    var r_eff: f32;
    if (len < 0.001) {
        r_eff = min(Rx, Ry);
    } else {
        dir = dir / len;
        let cx = dir.x / max(Rx, 0.0001);
        let cy = dir.y / max(Ry, 0.0001);
        let denom = sqrt(cx * cx + cy * cy);
        if (denom < 0.0001) {
            r_eff = 0.0;
        } else {
            r_eff = 1.0 / denom;
        }
    }

    let alpha_factor = fade_factor(d, r_eff);
    textureStore(outputTex, coord, vec4<f32>(color.rgb, color.a * alpha_factor));
}
