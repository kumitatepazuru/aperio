// マネージャー組み込みの通常合成。アイテムのブレンドモードの id が登録されていないときに使う。
import aperio_blend;

[shader("vertex")]
BlendVaryings vs_main(uint vertex_id : SV_VertexID) {
    return blend_vertex(vertex_id);
}

[shader("fragment")]
float4 fs_main(BlendVaryings v) : SV_Target {
    BlendFragment f;
    if (!blend_fetch(v, f)) {
        discard;
    }
    return blend_composite(f, f.src.rgb);
}
