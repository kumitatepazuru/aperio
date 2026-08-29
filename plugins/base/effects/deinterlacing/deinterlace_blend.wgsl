enable wgpu_binding_array;

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

// 端は最近傍にクランプする(README §7: 行-1←行0 / 行h←行h-1 の複製と同じ。
// 端の行だけ重みが 3/4 : 1/4 に寄るのもここから出る)。
fn load_row(tex: texture_2d<f32>, x: i32, y: i32, dims: vec2<i32>) -> vec4<f32> {
    return textureLoad(tex, vec2<i32>(x, clamp(y, 0, dims.y - 1)), 0);
}

// exedit-inspect deinterlacing README §6: `二重化`。全行に 1:2:1 の縦フィルタを掛ける。
// 1 - 2 + 1 = 0 なので縦のナイキスト(= 櫛そのもの)がきっちり消え、直流の利得は 1。
// フィールドで見れば自分のフィールドが 2/4、相手が 1/4 + 1/4 の 50:50 混合になる。
//
// 線形フィルタなので RGBA のまま掛けてよい ―― BT.601 変換は線形変換で、
// `worker_field` のような非線形の判定が無いため YCbCr で掛けた結果と一致する。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let up = load_row(tex, coord.x, coord.y - 1, dims);
    let mid = textureLoad(tex, coord, 0);
    let down = load_row(tex, coord.x, coord.y + 1, dims);

    textureStore(outputTex, coord, (up + mid * 2.0 + down) * 0.25);
}
