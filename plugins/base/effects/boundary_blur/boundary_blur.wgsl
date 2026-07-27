struct BoundaryBlurParams {
    radius: i32,
    aspect: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: BoundaryBlurParams;

const PI: f32 = 3.14159265358979323846;

// 実機の cos イーズテーブル table[i] = 2048*(1-cos(i*pi/4096)) を [0,1] へ
// 正規化した連続版(README 3.1)。
fn ease(x: f32) -> f32 {
    return 0.5 * (1.0 - cos(x * PI));
}

// 縁からの距離e(0=縁)を軸ランプ値(0..0.5)へ変換する。README 3.2の3フェーズ
// 書き込み(上端/中間ゼロ/下端)を1本の式にまとめたもので、e>=rで自動的に0になる。
fn axis_ramp(e: f32, r: f32) -> f32 {
    return max(r - e, 0.0) / (2.0 * r + 1.0);
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
    var Rx = R * min(1.0, 1.0 + A / 100.0);
    var Ry = R * min(1.0, 1.0 - A / 100.0);
    // README 2: rx/ry は独立に幅/高さの半分でクランプされる(hi/lo分割は無い)
    Rx = min(Rx, f32(W) * 0.5);
    Ry = min(Ry, f32(H) * 0.5);

    let dx = f32(min(coord.x, W - 1 - coord.x));
    let dy = f32(min(coord.y, H - 1 - coord.y));

    // README 3.2/3.3: 軸ごとのランプをイーズテーブルへ通し、ユークリッド和
    // (sqrt(v²+h²))を侵食量として元のアルファから線形に引く。
    let v = ease(axis_ramp(dy, Ry));
    let h = ease(axis_ramp(dx, Rx));
    let dist = sqrt(v * v + h * h);

    let color = textureLoad(tex, coord, 0);
    let alpha = max(color.a - 2.0 * dist, 0.0);
    textureStore(outputTex, coord, vec4<f32>(color.rgb, alpha));
}
