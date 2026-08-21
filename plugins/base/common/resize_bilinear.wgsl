enable wgpu_binding_array;

#import aperio::math::{bilinear4_load}

struct ResizeBilinearParams {
    out_width: i32,
    out_height: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: ResizeBilinearParams;

// bilinear4_loadによる汎用リサイズ(拡大・縮小どちらも同じ式でよい: srcは
// out_dimsに対するsrc_dimsの比率で決まるだけでスケール方向に依存しない)。
// テクセル中心基準の半画素シフトで写像する標準的なバイリニア。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let out_coord = vec2<i32>(global_id.xy);

    if (out_coord.x >= params.out_width || out_coord.y >= params.out_height) {
        return;
    }

    let tex = inputTex[0];
    let src_dims = vec2<i32>(textureDimensions(tex));
    let scale = vec2<f32>(src_dims) / vec2<f32>(f32(params.out_width), f32(params.out_height));

    let src = (vec2<f32>(out_coord) + vec2<f32>(0.5, 0.5)) * scale - vec2<f32>(0.5, 0.5);
    let color = bilinear4_load(tex, src);

    textureStore(outputTex, out_coord, color);
}
