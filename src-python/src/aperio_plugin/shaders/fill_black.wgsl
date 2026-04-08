@group(0) @binding(0)
var outputTex: texture_storage_2d<rgba32float, write>;

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let size = textureDimensions(outputTex);
    if gid.x >= size.x || gid.y >= size.y {
        return;
    }
    textureStore(outputTex, vec2<i32>(gid.xy), vec4<f32>(0.0, 0.0, 0.0, 1.0));
}
