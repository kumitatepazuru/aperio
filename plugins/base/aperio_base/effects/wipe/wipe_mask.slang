import aperio_bindings;

static const int PATTERN_CIRCLE = 0;
static const int PATTERN_SQUARE = 1;
static const int PATTERN_CLOCK = 2;
static const int PATTERN_HORIZONTAL = 3;
static const int PATTERN_VERTICAL = 4;

static const float TAU = 6.283185307179586;

struct WipeMaskParams {
    // 0=円 1=四角 2=時計 3=横 4=縦 (exedit-inspect wipe README §4 の ex_data.type)
    int pattern;
    // 可視領域の極性そのものを反転する(README §5-2)。時間方向の反転
    // (g = 1 - g)は python 側で適用済みなので、ここでは比較演算子の
    // 入れ替えだけを担う。
    int invert;
    // 進捗 g。原作の 0..4096(Q12)を 0.0..1.0 に正規化したもの。
    float progress;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<WipeMaskParams>> res;

// 進捗 g から「型」のマスクを描く(README §4)。型はオブジェクトと同じ大きさなので、
// 出力サイズは inputs.tex0 の寸法をそのまま使う(画素値そのものは読まない)。
// マスクは **alpha チャンネル** に入れる ―― こうすると後段のぼかしを
// common/box_blur_dir.slang(alpha加重の箱平均)にそのまま任せられる。
//
// 5パターンとも既定(未反転)で **g が増えると可視領域が増える**。ランプは`イン`で
// g: 0→1、`アウト`で g: 1→0 なので、素直に「インで現れ、アウトで消える」になる。
//
// 注意 ―― README §4 の「見つけた非対称性(円だけ既定の向きが逆)」は**誤り**なので
// 従わないこと。四角/時計/横/縦のワーカーは `setg` の結果を直後の2命令で反転して
// いる(四角 0x10090d0d / 時計 0x10090bc4 / 横 0x10090f63 / 縦 0x1009102f がすべて
// 同じ形):
//     setg dl            ; dl = (d1 > threshold)
//     dec  edx           ; 1 -> 0 / 0 -> 0xFFFFFFFF
//     and  edx, 0x1000   ; つまり setg が立つ側が「不可視」
// README はこの `dec`+`and` を見落として「setg 側が可視」と読んでいる。円だけ極性が
// 合っていたのは、円が setcc ではなく `test eax,eax; jg`(0x10090e68)の明示的な
// 算術分岐だったため。実機で5パターンの向きが揃っていることは確認済み。
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

    let w = float(dims.x);
    let h = float(dims.y);

    let g = res.params[0].progress;
    let inv = res.params[0].invert != 0;

    var mask: float = 0.0;

    if (res.params[0].pattern == PATTERN_CIRCLE) {
        // 5パターンで唯一アンチエイリアス帯を持つ。半径2乗を対角線長から作り、
        // 帯幅 band = 4*R を境界の階調に使う。
        //
        // 円だけ距離を「2倍座標」で測る ―― 原作は dx = 2x - w + 1 を列ごとに 2 ずつ
        // 進める(0x10090e5b の `sub ecx, ebp` と 0x10090e8b の `add ecx, 2`)。この
        // ため実効半径は sqrt(r2)/2 になり、g=1 でちょうど四隅に届く。等倍座標で
        // 書くと半径が2倍になり、g>=0.25 で全面が覆われてしまう(=アウトで変化が
        // 遅く始まり、反転では早く終わる)。中心も (dims-1)/2 で、他4パターンの
        // dims/2 とは別物。
        let dd = 2.0 * float2(coord) - float2(dims - int2(1, 1));
        let r2 = (w * w + h * h) * g;
        let band = 4.0 * sqrt(r2);
        var v: float = r2 - dot(dd, dd);
        if (inv) {
            v = dot(dd, dd) - r2;
        }
        // 原作の3分岐(v<=0 -> 0 / v>=band -> 満可視 / それ以外 v/band)と等価。
        // max() はゼロ除算防止(docs/plugin.md #5)。
        mask = clamp(v / max(band, 1e-6), 0.0, 1.0);
    } else {
        // 残り4パターンはハードエッジ(階調なし)。反転は論理否定ではなく
        // **比較演算子の入れ替え**(既定 `<=` / 反転 `>=`)で、境界ちょうどの画素は
        // どちらでも可視になる。
        //
        // 中心は原作と同じ整数除算(cx = w/2, cy = h/2)。w * 0.5 にすると
        // 偶数サイズで境界が半画素ずれる。
        let center = float2(dims / int2(2, 2));
        let d = float2(coord) - center;
        var visible: bool = false;

        if (res.params[0].pattern == PATTERN_SQUARE) {
            // 名前は「四角」だが実体は L1 距離のダイヤ形。
            let threshold = (center.x + center.y) * g;
            let d1 = abs(d.x) + abs(d.y);
            visible = d1 <= threshold;
            if (inv) {
                visible = d1 >= threshold;
            }
        } else if (res.params[0].pattern == PATTERN_CLOCK) {
            // 角度を0..1周に正規化し、0.25(原作の 0x4000 = 12時方向)を基準に
            // 測り直した掃引角と進捗を比べる。
            if (d.x == 0.0 && d.y == 0.0) {
                // 中心画素は角度が定義できないので、原作(0x10090b83)も atan2 を
                // 呼ばずに常に可視で埋める。
                visible = true;
            } else {
                let units = frac(-atan2(d.y, d.x) / TAU);
                let swept = frac(0.25 - units);
                visible = swept <= g;
                if (inv) {
                    visible = swept >= g;
                }
            }
        } else if (res.params[0].pattern == PATTERN_HORIZONTAL) {
            let threshold = w * g;
            visible = float(coord.x) <= threshold;
            if (inv) {
                visible = float(coord.x) >= threshold;
            }
        } else if (res.params[0].pattern == PATTERN_VERTICAL) {
            // 横と w<->h・x<->y を入れ替えただけの同一パターン。
            let threshold = h * g;
            visible = float(coord.y) <= threshold;
            if (inv) {
                visible = float(coord.y) >= threshold;
            }
        }

        mask = visible ? 1.0 : 0.0;
    }

    res.outputTex[coord] = float4(0.0, 0.0, 0.0, mask);
}
