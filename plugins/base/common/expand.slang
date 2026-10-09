import aperio_bindings;

struct ExpandParams {
    int offset_x;
    int offset_y;
    int out_width;
    int out_height;
    // 範囲外を埋める値。通常は透明(0,0,0,0)だが、色差アキュムレータのように
    // 「データが無い」ことを0以外の値(オフセット済みのニュートラル値)で表す
    // 必要がある場合に備えて呼び出し側から指定できるようにしてある。
    float fill_r;
    float fill_g;
    float fill_b;
    float fill_a;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<ExpandParams>> res;

// 元画像を、そのままの値で新しい(より大きい)キャンバスの
// (offset_x, offset_y)の位置に配置する。範囲外はfill_*で埋める。
// 発光がキャンバス外ににじみ出す分の余白を確保するために使う。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let out_coord = int2(global_id.xy);
    let out_dims = int2(res.params[0].out_width, res.params[0].out_height);

    if (out_coord.x >= out_dims.x || out_coord.y >= out_dims.y) {
        return;
    }

    let tex = inputs.tex0;
    uint dimsU, dimsV;
    tex.GetDimensions(dimsU, dimsV);
    let in_dims = int2(dimsU, dimsV);
    let in_coord = out_coord - int2(res.params[0].offset_x, res.params[0].offset_y);

    if (in_coord.x >= 0 && in_coord.x < in_dims.x && in_coord.y >= 0 && in_coord.y < in_dims.y) {
        res.outputTex[out_coord] = tex.Load(int3(in_coord, 0));
    } else {
        res.outputTex[out_coord] = float4(res.params[0].fill_r, res.params[0].fill_g, res.params[0].fill_b, res.params[0].fill_a);
    }
}
