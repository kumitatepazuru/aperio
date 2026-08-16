enable wgpu_binding_array;

const PI: f32 = 3.14159265358979323846;

struct DiagonalClippingParams {
    cx: f32,
    cy: f32,
    nx: f32,
    ny: f32,
    // ぼかし+1 [px]。階調帯の全幅(常に>=1、ハードエッジにはならない)
    band: f32,
    // 帯の全幅。符号でモードを選ぶ(0=半平面 />0=帯を残す /<0=帯を消す)
    width: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: DiagonalClippingParams;

// exedit-inspect diagonal_clipping README §6/§9 の3モードを、Q16固定小数点を経由せず
// 符号付き距離[px]のまま実装したもの。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    // 直線からの符号付き距離[px](README §4)。
    let d = params.nx * (f32(coord.x) - params.cx) + params.ny * (f32(coord.y) - params.cy);

    var m: f32;
    if (params.width == 0.0) {
        // 半平面: 階調は直線をまたいでcenteredに乗る。
        m = d + params.band * 0.5;
    } else if (params.width > 0.0) {
        // 帯を残す: |d| <= width/2 が可視。
        m = params.width * 0.5 - abs(d);
    } else {
        // 帯を消す: |d| <= |width|/2 が透明。
        m = abs(d) - abs(params.width) * 0.5;
    }

    var factor: f32;
    if (m >= params.band) {
        factor = 1.0;
    } else if (m <= 0.0) {
        factor = 0.0;
    } else {
        // 原作の1-cosイージング参照テーブル(exedit 0x101dcf78)の数式そのもの。
        let t = m / params.band;
        factor = (1.0 - cos(t * PI)) * 0.5;
    }

    let src = textureLoad(tex, coord, 0);
    textureStore(outputTex, coord, vec4<f32>(src.rgb, src.a * factor));
}
