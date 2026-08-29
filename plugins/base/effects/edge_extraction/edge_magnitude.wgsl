enable wgpu_binding_array;

#import aperio::color::{bt601_encode}

// 抽出モード。python側の {"luma":0, "alpha":1, "color":2} と対応する
// (exedit-inspect edge_extraction README §4の3ワーカー)。
const MODE_LUMA: i32 = 0;
const MODE_ALPHA: i32 = 1;
const MODE_COLOR: i32 = 2;

struct EdgeMagnitudeParams {
    mode: i32,
    // どちらもQ12の1.0(=UI表示の100)を1.0とする正規化値(README §3)。
    strength: f32,
    threshold: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: EdgeMagnitudeParams;

// exedit-inspect edge_extraction README §5: Prewittオペレータでエッジの強さを求め、
// 「中央画素のアルファ × エッジの強さ」だけを被覆率マップとして書き出す。
// 光色を載せる出力段は共通の common/encode_color.wgsl が担当する
// (`シャドー`・`縁取り` と同じ形)。rgbは常に0で、次段は.aしか読まない。
//
// Sobelではなく Prewitt(中央タップの重み2が無い)であることは README §5.1 が
// 命令列から係数行列を導出して確認している。
// Gx = [[-1,0,+1],[-1,0,+1],[-1,0,+1]]、Gy = [[-1,-1,-1],[0,0,0],[+1,+1,+1]] は
// そのままタップのオフセット(dx, dy)に一致するので、係数表を持たずに書ける。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    // README §6: 3×3の近傍が取れない外周1画素は、クランプもミラーもせず
    // 無条件に「光色 + アルファ0」で潰される。w<=2 / h<=2 のときに画像全体が
    // 外周になるのもこの判定で自然に再現される。
    if (coord.x == 0 || coord.y == 0 || coord.x == dims.x - 1 || coord.y == dims.y - 1) {
        textureStore(outputTex, coord, vec4<f32>(0.0));
        return;
    }

    // 読むのは8近傍だけ。中央画素の色は勾配に一切入らない(README §5.1)。
    var gx = vec3<f32>(0.0);
    var gy = vec3<f32>(0.0);
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            if (dx == 0 && dy == 0) {
                continue;
            }

            let s = textureLoad(tex, coord + vec2<i32>(dx, dy), 0);

            var v: vec3<f32>;
            if (params.mode == MODE_ALPHA) {
                // README §5.2: `透明度エッジ` にはプリマルチプライの分岐が無い ――
                // そこではアルファ自身がサンプルなので、掛けるものが無い。
                v = vec3<f32>(s.a, 0.0, 0.0);
            } else {
                // README §5.2: 各近傍の色を「その近傍自身のアルファ」で
                // プリマルチプライしてから畳み込む。実機の3分岐
                // (a>=4096 -> c / 0<a<4096 -> (c*a)>>12 / a<=0 -> 0)は
                // v = c * clamp(a, 0, 1) という1つの式に等しい(全数検算済み)。
                //
                // この帰結が大きい: 平坦な単色オブジェクトでもシルエットそのものが
                // エッジになる(アルファが落ちるところで y*a も一緒に落ちるため)。
                let ycc = bt601_encode(s.rgb) * clamp(s.a, 0.0, 1.0);
                if (params.mode == MODE_COLOR) {
                    v = ycc;
                } else {
                    v = vec3<f32>(ycc.z, 0.0, 0.0);
                }
            }

            gx += f32(dx) * v;
            gy += f32(dy) * v;
        }
    }

    // README §5.3: 平方根はチャンネルごとに取ってから足す。
    // √a + √b + √c >= √(a+b+c) なので、`色エッジ` はまとめて1回取るより早く飽和する。
    var mag = sqrt(gx.x * gx.x + gy.x * gy.x);
    if (params.mode == MODE_COLOR) {
        mag += sqrt(gx.y * gx.y + gy.y * gy.y) + sqrt(gx.z * gx.z + gy.z * gy.z);
    }

    // README §5.3: しきい値はゲートではなく引き算。負にすると全体が底上げされ、
    // 勾配が0の平坦部にもアルファが乗る。
    let e = clamp((mag - params.threshold) * params.strength, 0.0, 1.0);

    // README §5.3: 中央画素が登場する唯一の場所。完全に透明な領域の中で
    // 検出されたエッジはここで捨てられ、結果が元のシルエットの外へ出ることはない。
    let src_a = textureLoad(tex, coord, 0).a;
    textureStore(outputTex, coord, vec4<f32>(0.0, 0.0, 0.0, clamp(src_a * e, 0.0, 1.0)));
}
