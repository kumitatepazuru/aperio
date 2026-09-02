import aperio_bindings;

struct BoundaryBlurParams {
    int radius;
    int aspect;
};

struct InputTextures { Texture2D<float4> tex0; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<BoundaryBlurParams>> res;

static const float PI = 3.14159265358979323846;

// 実機の cos イーズテーブル table[i] = 2048*(1-cos(i*pi/4096)) を [0,1] へ
// 正規化した連続版(README 3.1)。
float ease(float x) {
    return 0.5 * (1.0 - cos(x * PI));
}

// 縁からの距離e(0=縁)を軸ランプ値(0..0.5)へ変換する。README 3.2の3フェーズ
// 書き込み(上端/中間ゼロ/下端)を1本の式にまとめたもので、e>=rで自動的に0になる。
float axis_ramp(float e, float r) {
    return max(r - e, 0.0) / (2.0 * r + 1.0);
}

[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let tex = inputs.tex0;
    uint dimsU, dimsV;
    tex.GetDimensions(dimsU, dimsV);
    let dims = int2(dimsU, dimsV);
    let W = dims.x;
    let H = dims.y;
    let coord = int2(global_id.xy);

    if (coord.x >= W || coord.y >= H) {
        return;
    }

    let R = float(res.params[0].radius);
    let A = float(res.params[0].aspect);
    var Rx: float = R * min(1.0, 1.0 + A / 100.0);
    var Ry: float = R * min(1.0, 1.0 - A / 100.0);
    // README 2: rx/ry は独立に幅/高さの半分でクランプされる(hi/lo分割は無い)
    Rx = min(Rx, float(W) * 0.5);
    Ry = min(Ry, float(H) * 0.5);

    let dx = float(min(coord.x, W - 1 - coord.x));
    let dy = float(min(coord.y, H - 1 - coord.y));

    // README 3.2/3.3: 軸ごとのランプをイーズテーブルへ通し、ユークリッド和
    // (sqrt(v²+h²))を侵食量として元のアルファから線形に引く。
    let v = ease(axis_ramp(dy, Ry));
    let h = ease(axis_ramp(dx, Rx));
    let dist = sqrt(v * v + h * h);

    let color = tex.Load(int3(coord, 0));
    let alpha = max(color.a - 2.0 * dist, 0.0);
    res.outputTex[coord] = float4(color.rgb, alpha);
}
