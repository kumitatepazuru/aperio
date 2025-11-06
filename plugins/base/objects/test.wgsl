struct Params {
    time: u32,
};

// パラメータは group1 binding0
@group(1) @binding(0) var<storage, read> params: Params;

// 出力は storage texture
@group(0) @binding(0)
var outputTex: texture_storage_2d<rgba32float, write>;

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let textureSize = textureDimensions(outputTex);

    let x = f32(global_id.x) / f32(textureSize.x);
    let y = f32(global_id.y) / f32(textureSize.y);

    let color = vec4<f32>(abs(sin(f32(params.time)/100.0)), x, y, 1.0);

    // storage texture へ書き込み
    textureStore(outputTex, vec2<i32>(global_id.xy), color);
}
