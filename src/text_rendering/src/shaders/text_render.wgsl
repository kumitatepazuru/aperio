// text_render.wgsl
// グリフアトラスから文字をインスタンシング描画するシェーダー

struct Uniforms {
    output_size: vec2<f32>,
    _pad: vec2<f32>,
}

@group(0) @binding(0) var t_atlas: texture_2d<f32>;
@group(0) @binding(1) var s_atlas: sampler;
@group(0) @binding(2) var<uniform> uniforms: Uniforms;

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vs_main(
    @builtin(vertex_index) vi: u32,
    // インスタンスデータ（VertexStepMode::Instance）
    @location(0) inst_pos:    vec2<f32>,  // 出力テクスチャ上のピクセル位置（左上）
    @location(1) inst_size:   vec2<f32>,  // グリフのピクセルサイズ
    @location(2) inst_uv_min: vec2<f32>,  // アトラス UV 左上（0-1 正規化）
    @location(3) inst_uv_max: vec2<f32>,  // アトラス UV 右下（0-1 正規化）
    @location(4) inst_color:  vec4<f32>,  // RGBA テキストカラー（0-1 正規化）
) -> VertexOut {
    // 2 三角形で四角形を描く（TRIANGLE_LIST, 6 頂点）
    var corners = array<vec2<f32>, 6>(
        vec2(0.0, 0.0),
        vec2(1.0, 0.0),
        vec2(0.0, 1.0),
        vec2(1.0, 0.0),
        vec2(1.0, 1.0),
        vec2(0.0, 1.0),
    );
    let c = corners[vi];
    let pixel = inst_pos + c * inst_size;
    // ピクセル座標 → NDC（Y 軸を反転）
    let ndc = vec2(
        pixel.x / uniforms.output_size.x * 2.0 - 1.0,
        1.0 - pixel.y / uniforms.output_size.y * 2.0,
    );

    var out: VertexOut;
    out.pos   = vec4(ndc, 0.0, 1.0);
    out.uv    = mix(inst_uv_min, inst_uv_max, c);
    out.color = inst_color;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let s = textureSample(t_atlas, s_atlas, in.uv);
    // マスクグリフ: s.rgb = (1,1,1),  s.a = カバレッジ → in.color で着色
    // カラーグリフ: s.rgba = 絵文字本来の色,  in.color = (1,1,1,alpha)
    // 統一式: output.rgb = s.rgb * color.rgb,  output.a = s.a * color.a
    return vec4(s.rgb * in.color.rgb, s.a * in.color.a);
}
