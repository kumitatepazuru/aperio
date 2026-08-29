enable wgpu_binding_array;

#import aperio::color::{bt601_encode, bt601_decode}

struct ConvexEdgeParams {
    steps: i32,
    // 単位方向ベクトル(exedit-inspect convex_edge README §3)。
    dir_x: f32,
    dir_y: f32,
    height: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: ConvexEdgeParams;

// exedit-inspect convex_edge README §4: 点対称な有限差分。ステップkごとの
// オフセットをfloor(k*dir)で整数化し、その符号反転を後方サンプルにそのまま使う
// (後方だけ別途floorし直さない)ことで点対称なサンプル対を保つ。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let src = textureLoad(tex, coord, 0);
    let ycc = bt601_encode(src.rgb);
    let cr = ycc.x;
    let cb = ycc.y;
    let y = ycc.z;

    var acc = vec2<f32>(0.0, 0.0);
    var sum = 0.0;
    let dir = vec2<f32>(params.dir_x, params.dir_y);
    for (var k = 1; k <= params.steps; k++) {
        acc += dir;
        let o = vec2<i32>(floor(acc));

        // README §4(c): 前後それぞれ独立に範囲判定する(枠外は寄与ゼロ)。
        let fwd = coord + o;
        if (fwd.x >= 0 && fwd.x < dims.x && fwd.y >= 0 && fwd.y < dims.y) {
            sum += textureLoad(tex, fwd, 0).a;
        }
        let back = coord - o;
        if (back.x >= 0 && back.x < dims.x && back.y >= 0 && back.y < dims.y) {
            sum -= textureLoad(tex, back, 0).a;
        }
    }

    // README §5: scale = 高さ_raw/(200*steps)。高さ(表示値) = 高さ_raw/100 を
    // 代入すると 200 が 2 に落ちる正規化空間での等価式。
    let scale = params.height / (2.0 * f32(params.steps));
    let d = sum * scale;

    var out_y: f32;
    var out_cb = cb;
    var out_cr = cr;
    if (d >= 0.0 || y <= 0.0) {
        // 明側: 加算のみ、クランプ無し(README §5)。
        out_y = y + d;
    } else {
        // 暗側: 下限0の1箇所だけクランプし、Y/Cb/Crを同じ係数で縮める(README §5)。
        let ny = max(0.0, y + d);
        let k = ny / y;
        out_y = ny;
        out_cb = cb * k;
        out_cr = cr * k;
    }

    let rgb = bt601_decode(out_cr, out_cb, out_y);
    textureStore(outputTex, coord, vec4<f32>(rgb, src.a));
}
