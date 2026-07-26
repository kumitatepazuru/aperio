struct AccumulateLumaCombinedParams {
    is_first: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: AccumulateLumaCombinedParams;

// 輝度・色差の6パスを1本化した経路(combined_init.wgsl)用の輝度アキュムレータ。
// inputTex[0]: このパスでぼかし済みのチェーン(.bに輝度量、カーブ適用後)
// inputTex[1]: これまでの輝度蓄積(.aに量の合計。is_first!=0のときは読まない)
//
// accumulate_luma.wgsl(輝度・色差を分けて回す経路用)と同じくクランプなし
// の単純加算(結合則が成り立つため、都度足し込む方式で最後にまとめて
// 加算するのと数学的に同じ)。読み出すチャンネルが.bになる点だけが異なる。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let dims = vec2<i32>(textureDimensions(inputTex[0]));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let contribution = textureLoad(inputTex[0], coord, 0).b;

    var old_accum = 0.0;
    if (params.is_first == 0) {
        old_accum = textureLoad(inputTex[1], coord, 0).a;
    }

    textureStore(outputTex, coord, vec4<f32>(0.0, 0.0, 0.0, contribution + old_accum));
}
