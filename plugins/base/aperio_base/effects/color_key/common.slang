module common;

// exedit-inspect color_key README §4: 距離ではなく軸並行な直方体、6境界はすべて閉区間。
// キー色の直方体内に入っていればアルファ0、外ならsrc_alphaをそのまま返す(2値、falloffなし)。
public float key_alpha(
    float y, float cb, float cr,
    float key_y, float key_cb, float key_cr,
    float luma_range, float chroma_range,
    float src_alpha
) {
    let in_y = y >= key_y - luma_range && y <= key_y + luma_range;
    let in_cb = cb >= key_cb - chroma_range && cb <= key_cb + chroma_range;
    let in_cr = cr >= key_cr - chroma_range && cr <= key_cr + chroma_range;
    if (in_y && in_cb && in_cr) {
        return 0.0;
    }
    return src_alpha;
}
