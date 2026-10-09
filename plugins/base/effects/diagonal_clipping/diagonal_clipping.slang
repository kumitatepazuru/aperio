import aperio_bindings;

static const float PI = 3.14159265358979323846;

struct DiagonalClippingParams {
    float cx;
    float cy;
    float nx;
    float ny;
    // ぼかし+1 [px]。階調帯の全幅(常に>=1、ハードエッジにはならない)
    float band;
    // 帯の全幅。符号でモードを選ぶ(0=半平面 />0=帯を残す /<0=帯を消す)
    float width;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<DiagonalClippingParams>> res;

// exedit-inspect diagonal_clipping README §6/§9 の3モードを、Q16固定小数点を経由せず
// 符号付き距離[px]のまま実装したもの。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let tex = inputs.tex0;
    uint dimsU, dimsV;
    tex.GetDimensions(dimsU, dimsV);
    let dims = int2(dimsU, dimsV);
    let coord = int2(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    // 直線からの符号付き距離[px](README §4)。
    let d = res.params[0].nx * (float(coord.x) - res.params[0].cx) + res.params[0].ny * (float(coord.y) - res.params[0].cy);

    var m: float;
    if (res.params[0].width == 0.0) {
        // 半平面: 階調は直線をまたいでcenteredに乗る。
        m = d + res.params[0].band * 0.5;
    } else if (res.params[0].width > 0.0) {
        // 帯を残す: |d| <= width/2 が可視。
        m = res.params[0].width * 0.5 - abs(d);
    } else {
        // 帯を消す: |d| <= |width|/2 が透明。
        m = abs(d) - abs(res.params[0].width) * 0.5;
    }

    var factor: float;
    if (m >= res.params[0].band) {
        factor = 1.0;
    } else if (m <= 0.0) {
        factor = 0.0;
    } else {
        // 原作の1-cosイージング参照テーブル(exedit 0x101dcf78)の数式そのもの。
        let t = m / res.params[0].band;
        factor = (1.0 - cos(t * PI)) * 0.5;
    }

    let src = tex.Load(int3(coord, 0));
    res.outputTex[coord] = float4(src.rgb, src.a * factor);
}
