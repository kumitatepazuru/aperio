import aperio_color;
import aperio_bindings;

// 抽出モード。python側の {"luma":0, "alpha":1, "color":2} と対応する
// (exedit-inspect edge_extraction README §4の3ワーカー)。
static const int MODE_LUMA = 0;
static const int MODE_ALPHA = 1;
static const int MODE_COLOR = 2;

struct EdgeMagnitudeParams {
    int mode;
    // どちらもQ12の1.0(=UI表示の100)を1.0とする正規化値(README §3)。
    float strength;
    float threshold;
};

struct InputTextures { Texture2D<float4> tex0; };
ParameterBlock<InputTextures> inputs;
ParameterBlock<EffectResources<EdgeMagnitudeParams>> res;

// exedit-inspect edge_extraction README §5: Prewittオペレータでエッジの強さを求め、
// 「中央画素のアルファ × エッジの強さ」だけを被覆率マップとして書き出す。
// 光色を載せる出力段は共通の common/encode_color.slang が担当する
// (`シャドー`・`縁取り` と同じ形)。rgbは常に0で、次段は.aしか読まない。
//
// Sobelではなく Prewitt(中央タップの重み2が無い)であることは README §5.1 が
// 命令列から係数行列を導出して確認している。
// Gx = [[-1,0,+1],[-1,0,+1],[-1,0,+1]]、Gy = [[-1,-1,-1],[0,0,0],[+1,+1,+1]] は
// そのままタップのオフセット(dx, dy)に一致するので、係数表を持たずに書ける。
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

    // README §6: 3×3の近傍が取れない外周1画素は、クランプもミラーもせず
    // 無条件に「光色 + アルファ0」で潰される。w<=2 / h<=2 のときに画像全体が
    // 外周になるのもこの判定で自然に再現される。
    if (coord.x == 0 || coord.y == 0 || coord.x == dims.x - 1 || coord.y == dims.y - 1) {
        res.outputTex[coord] = float4(0.0, 0.0, 0.0, 0.0);
        return;
    }

    // 読むのは8近傍だけ。中央画素の色は勾配に一切入らない(README §5.1)。
    var gx: float3 = float3(0.0, 0.0, 0.0);
    var gy: float3 = float3(0.0, 0.0, 0.0);
    for (int dy = -1; dy <= 1; dy++) {
        for (int dx = -1; dx <= 1; dx++) {
            if (dx == 0 && dy == 0) {
                continue;
            }

            let s = tex.Load(int3(coord + int2(dx, dy), 0));

            var v: float3;
            if (res.params[0].mode == MODE_ALPHA) {
                // README §5.2: `透明度エッジ` にはプリマルチプライの分岐が無い ――
                // そこではアルファ自身がサンプルなので、掛けるものが無い。
                v = float3(s.a, 0.0, 0.0);
            } else {
                // README §5.2: 各近傍の色を「その近傍自身のアルファ」で
                // プリマルチプライしてから畳み込む。実機の3分岐
                // (a>=4096 -> c / 0<a<4096 -> (c*a)>>12 / a<=0 -> 0)は
                // v = c * clamp(a, 0, 1) という1つの式に等しい(全数検算済み)。
                //
                // この帰結が大きい: 平坦な単色オブジェクトでもシルエットそのものが
                // エッジになる(アルファが落ちるところで y*a も一緒に落ちるため)。
                let ycc = bt601_encode(s.rgb) * clamp(s.a, 0.0, 1.0);
                if (res.params[0].mode == MODE_COLOR) {
                    v = ycc;
                } else {
                    v = float3(ycc.z, 0.0, 0.0);
                }
            }

            gx += float(dx) * v;
            gy += float(dy) * v;
        }
    }

    // README §5.3: 平方根はチャンネルごとに取ってから足す。
    // √a + √b + √c >= √(a+b+c) なので、`色エッジ` はまとめて1回取るより早く飽和する。
    var mag = sqrt(gx.x * gx.x + gy.x * gy.x);
    if (res.params[0].mode == MODE_COLOR) {
        mag += sqrt(gx.y * gx.y + gy.y * gy.y) + sqrt(gx.z * gx.z + gy.z * gy.z);
    }

    // README §5.3: しきい値はゲートではなく引き算。負にすると全体が底上げされ、
    // 勾配が0の平坦部にもアルファが乗る。
    let e = clamp((mag - res.params[0].threshold) * res.params[0].strength, 0.0, 1.0);

    // README §5.3: 中央画素が登場する唯一の場所。完全に透明な領域の中で
    // 検出されたエッジはここで捨てられ、結果が元のシルエットの外へ出ることはない。
    let src_a = tex.Load(int3(coord, 0)).a;
    res.outputTex[coord] = float4(0.0, 0.0, 0.0, clamp(src_a * e, 0.0, 1.0));
}
