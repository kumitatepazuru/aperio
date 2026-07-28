enable wgpu_binding_array;

struct ExpandParams {
    offset_x: i32,
    offset_y: i32,
    out_width: i32,
    out_height: i32,
    // 範囲外を埋める値。通常は透明(0,0,0,0)だが、色差アキュムレータのように
    // 「データが無い」ことを0以外の値(オフセット済みのニュートラル値)で表す
    // 必要がある場合に備えて呼び出し側から指定できるようにしてある。
    fill_r: f32,
    fill_g: f32,
    fill_b: f32,
    fill_a: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: ExpandParams;

// 元画像を、そのままの値で新しい(より大きい)キャンバスの
// (offset_x, offset_y)の位置に配置する。範囲外はfill_*で埋める。
// 発光がキャンバス外ににじみ出す分の余白を確保するために使う。
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
    } else {
        textureStore(outputTex, out_coord, vec4<f32>(params.fill_r, params.fill_g, params.fill_b, params.fill_a));
    }
}
