import aperio_color;
import aperio_bindings;

struct GlintParams {
    // 拡張後キャンバスの左上が、元オブジェクト座標のどこにあたるか(常に<=0)
    int x0;
    int y0;
    int out_width;
    int out_height;
    // 光の中心(元オブジェクト座標)
    int cx;
    int cy;
    // サンプル数上限 R = trunc(maxd/4) + trunc(maxd/2)
    int sample_cap;
    // しきい値 T = 1 - 強さ。実機の T = 4096 - strength を4096で割ったもの
    float threshold;
    // 1 = 光色「指定なし」(元画素自身の色でサンプルする)
    // 0 = 光色指定(ソースのアルファだけを読み、指定色に掛ける)
    int use_source_color;
    float color_r;
    float color_g;
    float color_b;
    // 0 = 前方に合成(加算)、1 = 後方に合成(通常)、2 = 光成分のみ
    int mode;
};

struct InputTextures { Texture2D<float4> tex0; };
[vk::binding(0, 0)]
ParameterBlock<InputTextures> inputs;
[vk::binding(0, 1)]
ParameterBlock<EffectResources<GlintParams>> res;

// 閃光の光線ワーカー(実機 0x1004e9c0 / 0x1004ee20、README手順4・5)。
//
// 出力画素ごとに中心へ向かう光線を走らせ、中心までの距離の75%区間を等間隔に
// サンプリングしてプリマルチプライド平均を取る。実機の2つのワーカーの違いは
// 「サンプルする色が元画素自身か指定色か」だけなので use_source_color で1本に
// 統合してある。
//
// 整数量(dist8 / n / len)は実機と同じく int のまま計算する。Slang(HLSL系)の
// int 除算は C と同じ0方向切り捨てなので、実機の msvc_div / c_div とそのまま一致する。
//
// 蓄積と出力は float の RGB 空間で行う。実機は YCbCr の (y, cb, cr) を別々に
// 扱うが、最終エンコードで3成分に掛かるのは一様なスケールだけなので、線形変換で
// ある BT.601 を挟まず RGB のまま計算しても同値になる(輝度の算出にだけ BT.601 の
// 係数を使う)。実機の16bit書き戻しの折り返しは float では起きないので再現しない。
//
// 中心画素の経路(README手順6)は再現しない。実機は未初期化のスタックスロットを
// アルファとして読む既知の不具合で、値がスレッド分割と走査順に依存するため GPU
// では意味を成さない。リファレンス実装の「まだ何もサンプルしていなければ0」と
// 同じく透明を書く(影響は中心が乗る1画素だけ)。
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 global_id : SV_DispatchThreadID) {
    let out_coord = int2(global_id.xy);
    if (out_coord.x >= res.params[0].out_width || out_coord.y >= res.params[0].out_height) {
        return;
    }

    let src = inputs.tex0;
    uint srcDimsU, srcDimsV;
    src.GetDimensions(srcDimsU, srcDimsV);
    let src_dims = int2(srcDimsU, srcDimsV);

    // 元オブジェクト基準の座標へ戻す
    let x = out_coord.x + res.params[0].x0;
    let y = out_coord.y + res.params[0].y0;
    let dx = res.params[0].cx - x;
    let dy = res.params[0].cy - y;

    let dxf = float(dx);
    let dyf = float(dy);
    let dist = sqrt(dxf * dxf + dyf * dyf);
    let dist8 = int(dist * 8.0);          // 1/8画素単位。int変換 = 0方向切り捨て = _ftol
    var len = dist8 * 750 / 1000;         // 走る距離は常に 0.75 * dist
    var n: int = 0;

    if (dist8 > res.params[0].sample_cap) {
        // 遠い画素 ― サンプル数を R で頭打ちにする。サンプル間隔が1画素を超えると
        // 光条は破線になる(README手順4の表)。
        n = res.params[0].sample_cap;
        len = len * res.params[0].sample_cap / dist8;
    } else {
        // 中心近傍 ― 逆にスーパーサンプルする
        var m: int = 1;
        if (dist8 > 8) {
            m = 8;
        } else if (dist8 > 4) {
            m = 4;
        } else if (dist8 > 2) {
            m = 2;
        }
        n = dist8 * m;
        len = len * m;
    }

    // 中心画素の経路。実機の借り物アルファは再現せず透明にする。以降、光の色が
    // 求まらない経路はすべてこの light=0 のまま後段の合成へ進む(旧実装は
    // ここでtextureStoreして即returnしていたが、mode!=2ではその画素にも
    // 元オブジェクトだけは描く必要があるため、早期returnはできない)。
    var light: float4 = float4(0.0, 0.0, 0.0, 0.0);

    if (n >= 2 && len >= 2) {
        // 実機は16.16固定小数でステップを切り捨ててから累算するが、ここでは float の
        // ステップを i 倍して求める(累積誤差が乗らないぶんこちらの方が素直で、
        // 差はサンプル位置で高々0.1画素未満)。
        let ray_step = float2(dxf, dyf) / float(n);
        let start = float2(float(x), float(y)) + 0.5;

        // 光線と元画像矩形の交差だけをループする。結果は変わらない純粋な最適化で、
        // これが無いと拡張キャンバス外周の画素が「1サンプルも当たらない光線」を最後まで
        // 回してしまう。浮動小数の丸めで1サンプルずれても壊れないよう範囲は±1広げ、
        // 実際の範囲判定はループ内に残してある。
        let src_size = float2(src_dims);
        // ステップが極端に小さいと t が int に収まらない大きさになるので、
        // 変換前に [-1, len+1] へ丸めておく(どのみち下で [0, len] に潰される)。
        let t_min = -1.0;
        let t_max = float(len) + 1.0;
        var lo: int = 0;
        var hi: int = len;

        if (abs(ray_step.x) < 1e-9) {
            if (start.x < 0.0 || start.x >= src_size.x) {
                hi = 0;
            }
        } else {
            let tx0 = (0.0 - start.x) / ray_step.x;
            let tx1 = (src_size.x - start.x) / ray_step.x;
            lo = max(lo, int(floor(clamp(min(tx0, tx1), t_min, t_max))) - 1);
            hi = min(hi, int(ceil(clamp(max(tx0, tx1), t_min, t_max))) + 1);
        }

        if (abs(ray_step.y) < 1e-9) {
            if (start.y < 0.0 || start.y >= src_size.y) {
                hi = 0;
            }
        } else {
            let ty0 = (0.0 - start.y) / ray_step.y;
            let ty1 = (src_size.y - start.y) / ray_step.y;
            lo = max(lo, int(floor(clamp(min(ty0, ty1), t_min, t_max))) - 1);
            hi = min(hi, int(ceil(clamp(max(ty0, ty1), t_min, t_max))) + 1);
        }

        let light_color = float3(res.params[0].color_r, res.params[0].color_g, res.params[0].color_b);
        var acc: float3 = float3(0.0, 0.0, 0.0);
        for (int i = lo; i < hi; i++) {
            let p = start + ray_step * float(i);
            let c = int2(floor(p));
            if (c.x < 0 || c.x >= src_dims.x || c.y < 0 || c.y >= src_dims.y) {
                continue;
            }
            let s = src.Load(int3(c, 0));
            // 実機の (c * a) >> 12 = プリマルチプライ。範囲外・アルファ0のサンプルは
            // 読み飛ばすだけで、割る数は常に len のまま(黒として平均されるのと同じ)。
            if (res.params[0].use_source_color != 0) {
                acc += s.rgb * s.a;
            } else {
                acc += light_color * s.a;
            }
        }

        let avg = acc / float(len);
        let avg_y = bt601_luma(avg);

        // しきい値。実機は色差にも (avgY-T)/avgY を掛けてから 4096/(avgY-T) を掛けるが、
        // 掛け合わせると T が完全に約分されるので、残るのは「輝度で割って明るさを
        // アルファへ移すアンプリマルチプライ」だけになる(README手順5)。
        let out_a = avg_y - res.params[0].threshold;
        if (out_a > 0.0 && avg_y > 0.0) {
            // out_a >= 1 ではアルファが飽和し、超過分は輝度側が持つ(超白)。
            //
            // ここで rgb を [0,1] にクランプしてはいけない。アンプリマルチプライは
            // 「輝度で割って明るさをアルファへ移す」変換なので、彩度の高い色ほど
            // ストレート色はガモット外へ出るのが正常な状態で(純赤なら 1/0.299 = 3.34 倍)、
            // 合成時にアルファを掛け戻したところで初めて元のプリマルチプライド値に戻る。
            // 潰すと掛け戻しが効かず、色指定時の光が実機よりはっきり薄くなる。実機も
            // YC のまま合成して、RGB へのクリップは最後の表示時にしか行わない。
            let rgb = avg / avg_y * max(out_a, 1.0);
            light = float4(rgb, min(out_a, 1.0));
        }
    }

    if (res.params[0].mode == 2) {
        res.outputTex[out_coord] = light;
        return;
    }

    // 元オブジェクトを、拡張後の正しい位置に置き直す(旧common/expand.wgsl相当)。
    // このシェーダーは冒頭で既に元オブジェクト座標(x, y)を求めており、これは
    // expand.slangの逆写像 in_coord = out_coord - offset (offset=(-x0,-y0)) と
    // 完全に一致するので、追加の入力テクスチャなしに同じsrcから直接読める。
    var base: float4 = float4(0.0, 0.0, 0.0, 0.0);
    if (x >= 0 && x < src_dims.x && y >= 0 && y < src_dims.y) {
        base = src.Load(int3(x, y, 0));
    }

    // 光のバッファの上に元オブジェクトを描き直す合成(旧merge.wgsl)。
    // 前方に合成 = 加算、後方に合成 = 通常合成。
    var out_alpha: float = 0.0;
    var premul: float3 = float3(0.0, 0.0, 0.0);
    if (res.params[0].mode == 0) {
        out_alpha = base.a + light.a - base.a * light.a;
        premul = base.rgb * base.a + light.rgb * light.a;
    } else {
        out_alpha = base.a + light.a * (1.0 - base.a);
        premul = base.rgb * base.a + light.rgb * light.a * (1.0 - base.a);
    }

    // 光のストレート色はガモット外に出ていることがある(上のコメント参照)。
    // ここでクランプすると掛け戻しが効かなくなるので、プリマルチプライドで合成して
    // 割り戻すだけにする。RGBへのクリップは実機と同じく最後の表示側に任せる。
    var out_rgb: float3 = float3(0.0, 0.0, 0.0);
    if (out_alpha > 1e-6) {
        out_rgb = premul / out_alpha;
    }

    res.outputTex[out_coord] = float4(out_rgb, clamp(out_alpha, 0.0, 1.0));
}
