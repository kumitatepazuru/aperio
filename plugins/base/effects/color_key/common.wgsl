#define_import_path aperio::color_key

// exedit-inspect color_key README §4: 距離ではなく軸並行な直方体、6境界はすべて閉区間。
// キー色の直方体内に入っていればアルファ0、外ならsrc_alphaをそのまま返す(2値、falloffなし)。
fn key_alpha(
    y: f32, cb: f32, cr: f32,
    key_y: f32, key_cb: f32, key_cr: f32,
    luma_range: f32, chroma_range: f32,
    src_alpha: f32,
) -> f32 {
    let in_y = y >= key_y - luma_range && y <= key_y + luma_range;
    let in_cb = cb >= key_cb - chroma_range && cb <= key_cb + chroma_range;
    let in_cr = cr >= key_cr - chroma_range && cr <= key_cr + chroma_range;
    if (in_y && in_cb && in_cr) {
        return 0.0;
    }
    return src_alpha;
}
