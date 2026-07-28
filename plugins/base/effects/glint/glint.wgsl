enable wgpu_binding_array;

struct GlintParams {
    // 拡張後キャンバスの左上が、元オブジェクト座標のどこにあたるか(常に<=0)
    x0: i32,
    y0: i32,
    out_width: i32,
    out_height: i32,
    // 光の中心(元オブジェクト座標)
    cx: i32,
    cy: i32,
    // サンプル数上限 R = trunc(maxd/4) + trunc(maxd/2)
    sample_cap: i32,
    // しきい値 T = 1 - 強さ。実機の T = 4096 - strength を4096で割ったもの
    threshold: f32,
    // 1 = 光色「指定なし」(元画素自身の色でサンプルする)
    // 0 = 光色指定(ソースのアルファだけを読み、指定色に掛ける)
    use_source_color: i32,
    color_r: f32,
    color_g: f32,
    color_b: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: GlintParams;

// 閃光の光線ワーカー(実機 0x1004e9c0 / 0x1004ee20、README手順4・5)。
//
// 出力画素ごとに中心へ向かう光線を走らせ、中心までの距離の75%区間を等間隔に
// サンプリングしてプリマルチプライド平均を取る。実機の2つのワーカーの違いは
// 「サンプルする色が元画素自身か指定色か」だけなので use_source_color で1本に
// 統合してある。
//
// 整数量(dist8 / n / len)は実機と同じく i32 のまま計算する。WGSL の i32 除算は
// C と同じ0方向切り捨てなので、実機の msvc_div / c_div とそのまま一致する。
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
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let out_coord = vec2<i32>(global_id.xy);
    if (out_coord.x >= params.out_width || out_coord.y >= params.out_height) {
        return;
    }

    let src = inputTex[0];
    let src_dims = vec2<i32>(textureDimensions(src));

    // 元オブジェクト基準の座標へ戻す
    let x = out_coord.x + params.x0;
    let y = out_coord.y + params.y0;
    let dx = params.cx - x;
    let dy = params.cy - y;

    let dxf = f32(dx);
    let dyf = f32(dy);
    let dist = sqrt(dxf * dxf + dyf * dyf);
    let dist8 = i32(dist * 8.0);          // 1/8画素単位。i32変換 = 0方向切り捨て = _ftol
    var len = dist8 * 750 / 1000;         // 走る距離は常に 0.75 * dist
    var n = 0;

    if (dist8 > params.sample_cap) {
        // 遠い画素 ― サンプル数を R で頭打ちにする。サンプル間隔が1画素を超えると
        // 光条は破線になる(README手順4の表)。
        n = params.sample_cap;
        len = len * params.sample_cap / dist8;
    } else {
        // 中心近傍 ― 逆にスーパーサンプルする
        var m = 1;
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

    // 中心画素の経路。実機の借り物アルファは再現せず透明にする
    if (n < 2 || len < 2) {
        textureStore(outputTex, out_coord, vec4<f32>(0.0));
        return;
    }

    // 実機は16.16固定小数でステップを切り捨ててから累算するが、ここでは float の
    // ステップを i 倍して求める(累積誤差が乗らないぶんこちらの方が素直で、
    // 差はサンプル位置で高々0.1画素未満)。
    let ray_step = vec2<f32>(dxf, dyf) / f32(n);
    let start = vec2<f32>(f32(x), f32(y)) + 0.5;

    // 光線と元画像矩形の交差だけをループする。結果は変わらない純粋な最適化で、
    // これが無いと拡張キャンバス外周の画素が「1サンプルも当たらない光線」を最後まで
    // 回してしまう。浮動小数の丸めで1サンプルずれても壊れないよう範囲は±1広げ、
    // 実際の範囲判定はループ内に残してある。
    let src_size = vec2<f32>(src_dims);
    // ステップが極端に小さいと t が i32 に収まらない大きさになるので、
    // 変換前に [-1, len+1] へ丸めておく(どのみち下で [0, len] に潰される)。
    let t_min = -1.0;
    let t_max = f32(len) + 1.0;
    var lo = 0;
    var hi = len;

    if (abs(ray_step.x) < 1e-9) {
        if (start.x < 0.0 || start.x >= src_size.x) {
            hi = 0;
        }
    } else {
        let tx0 = (0.0 - start.x) / ray_step.x;
        let tx1 = (src_size.x - start.x) / ray_step.x;
        lo = max(lo, i32(floor(clamp(min(tx0, tx1), t_min, t_max))) - 1);
        hi = min(hi, i32(ceil(clamp(max(tx0, tx1), t_min, t_max))) + 1);
    }

    if (abs(ray_step.y) < 1e-9) {
        if (start.y < 0.0 || start.y >= src_size.y) {
            hi = 0;
        }
    } else {
        let ty0 = (0.0 - start.y) / ray_step.y;
        let ty1 = (src_size.y - start.y) / ray_step.y;
        lo = max(lo, i32(floor(clamp(min(ty0, ty1), t_min, t_max))) - 1);
        hi = min(hi, i32(ceil(clamp(max(ty0, ty1), t_min, t_max))) + 1);
    }

    let light_color = vec3<f32>(params.color_r, params.color_g, params.color_b);
    var acc = vec3<f32>(0.0);
    for (var i = lo; i < hi; i++) {
        let p = start + ray_step * f32(i);
        let c = vec2<i32>(floor(p));
        if (c.x < 0 || c.x >= src_dims.x || c.y < 0 || c.y >= src_dims.y) {
            continue;
        }
        let s = textureLoad(src, c, 0);
        // 実機の (c * a) >> 12 = プリマルチプライ。範囲外・アルファ0のサンプルは
        // 読み飛ばすだけで、割る数は常に len のまま(黒として平均されるのと同じ)。
        if (params.use_source_color != 0) {
            acc += s.rgb * s.a;
        } else {
            acc += light_color * s.a;
        }
    }

    let avg = acc / f32(len);
    let avg_y = dot(avg, vec3<f32>(0.299, 0.587, 0.114));

    // しきい値。実機は色差にも (avgY-T)/avgY を掛けてから 4096/(avgY-T) を掛けるが、
    // 掛け合わせると T が完全に約分されるので、残るのは「輝度で割って明るさを
    // アルファへ移すアンプリマルチプライ」だけになる(README手順5)。
    let out_a = avg_y - params.threshold;
    if (out_a <= 0.0 || avg_y <= 0.0) {
        textureStore(outputTex, out_coord, vec4<f32>(0.0));
        return;
    }

    // out_a >= 1 ではアルファが飽和し、超過分は輝度側が持つ(超白)。
    //
    // ここで rgb を [0,1] にクランプしてはいけない。アンプリマルチプライは
    // 「輝度で割って明るさをアルファへ移す」変換なので、彩度の高い色ほど
    // ストレート色はガモット外へ出るのが正常な状態で(純赤なら 1/0.299 = 3.34 倍)、
    // 合成時にアルファを掛け戻したところで初めて元のプリマルチプライド値に戻る。
    // 潰すと掛け戻しが効かず、色指定時の光が実機よりはっきり薄くなる。実機も
    // YC のまま合成して、RGB へのクリップは最後の表示時にしか行わない。
    let rgb = avg / avg_y * max(out_a, 1.0);
    textureStore(outputTex, out_coord, vec4<f32>(rgb, min(out_a, 1.0)));
}
