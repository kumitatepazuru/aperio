enable wgpu_binding_array;

struct TileParams {
    // タイルの位相シフト(exedit-inspect composite_image README §5:
    // 「負の座標をx%dw/y%dhで畳んだ位置から始めて敷き詰める」)。
    offset_x: i32,
    offset_y: i32,
    out_width: i32,
    out_height: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: TileParams;

// パターン画像(inputTex[0])を出力サイズいっぱいに単純タイルする(補間なし)。
// タイル原点は常に(0,0)。`シャドー`(exedit-inspect shadow README §6。影の箱の
// 左上が原点)と`縁取り`(exedit-inspect border README §6。キャンバスの左上が
// 原点)は原点の取り方が一致するため共通化できる。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let out_coord = vec2<i32>(global_id.xy);

    if (out_coord.x >= params.out_width || out_coord.y >= params.out_height) {
        return;
    }

    let tex = inputTex[0];
    let pattern_dims = vec2<i32>(textureDimensions(tex));
    let shifted = out_coord - vec2<i32>(params.offset_x, params.offset_y);
    let in_coord = ((shifted % pattern_dims) + pattern_dims) % pattern_dims;

    textureStore(outputTex, out_coord, textureLoad(tex, in_coord, 0));
}
