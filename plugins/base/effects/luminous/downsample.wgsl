struct DownsampleParams {
    factor: i32,
    out_width: i32,
    out_height: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: DownsampleParams;

// 「高速化」ONの近似ぼかし用: 元画像を factor×factor の窓で単純平均して
// 1/factor に縮小する。以降の大半径ボックスぼかしをこの縮小画像上で行うことで
// タップ数を factor^2 分の1に抑える。RGBA一括で平均する(色差offset・輝度・
// a=1 はいずれも線形なので保存される)。窓が画像端をはみ出す部分は数に入れず、
// 実際に存在する画素だけの平均を取る(縮小前の box が範囲外を0扱いするのと同じ、
// 端に張り付いた偏りを避ける)。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let out_coord = vec2<i32>(global_id.xy);
    if (out_coord.x >= params.out_width || out_coord.y >= params.out_height) {
        return;
    }

    let tex = inputTex[0];
    let in_dims = vec2<i32>(textureDimensions(tex));
    let base = out_coord * params.factor;

    var acc = vec4<f32>(0.0);
    var count = 0.0;
    for (var dy = 0; dy < params.factor; dy++) {
        for (var dx = 0; dx < params.factor; dx++) {
            let p = base + vec2<i32>(dx, dy);
            if (p.x < in_dims.x && p.y < in_dims.y) {
                acc += textureLoad(tex, p, 0);
                count += 1.0;
            }
        }
    }

    var result = vec4<f32>(0.0);
    if (count > 0.0) {
        result = acc / count;
    }
    textureStore(outputTex, out_coord, result);
}
