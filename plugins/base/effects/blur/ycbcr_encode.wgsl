@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

// 光の強さ>0のときだけ使う前処理。RGB(ストレートアルファ)をBT.601の
// 輝度Y・色差Cr/Cbへ変換し、{r=Cr偏差, g=Cb偏差, b=Y, a=alpha} に詰め替える
// (README手順6)。alphaは素通し。カーブ自体はこの後段のcommon/curve.wgsl
// (channel=2)が担当する。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let s = textureLoad(tex, coord, 0);
    let y = dot(s.rgb, vec3<f32>(0.299, 0.587, 0.114));
    let cr = (s.r - y) / 1.402000;
    let cb = (s.b - y) / 1.772000;

    textureStore(outputTex, coord, vec4<f32>(cr, cb, y, s.a));
}
