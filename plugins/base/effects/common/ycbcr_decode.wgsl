enable wgpu_binding_array;

#import aperio::color::{bt601_decode}

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

// ycbcr_encode.wgsl の逆変換。{r=Cr偏差, g=Cb偏差, b=Y} からBT.601でRGBを
// 復元する。alphaは素通し。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let s = textureLoad(tex, coord, 0);
    let rgb = bt601_decode(s.r, s.g, s.b);

    textureStore(outputTex, coord, vec4<f32>(rgb, s.a));
}
