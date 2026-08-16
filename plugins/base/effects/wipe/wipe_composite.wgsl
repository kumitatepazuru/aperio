enable wgpu_binding_array;

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

// 最終合成(exedit-inspect wipe README §7): objAlpha *= patternAlpha。
// inputTex[0] = 元のオブジェクト、inputTex[1] = wipe_mask.wgsl が描いた(そして
// ぼかしを通った)パターンマスク。色(rgb)は一度も読み替えないので、ワイプは
// `フェード`同様アルファを減らすことしかできない。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let src = textureLoad(tex, coord, 0);
    let mask = textureLoad(inputTex[1], coord, 0).a;

    textureStore(outputTex, coord, vec4<f32>(src.rgb, src.a * mask));
}
