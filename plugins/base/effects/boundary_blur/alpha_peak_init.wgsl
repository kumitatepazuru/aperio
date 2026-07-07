@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

// JFAで求めた「最寄りの透明ピクセル座標」から、各ピクセルの不透明境界までの距離Dを求め、
// 3x3近傍のうちDが最大のものへ向かう「ポインタ」を1歩分だけ仕込む。
// これを後続のポインタジャンプパスで繰り返し辿ることで、連結した不透明領域(島)ごとに
// Dが最大となる点(=その島の最も奥まった中心点)へ収束していく。
// 厳密な「より大きい」場合のみポインタを更新するため、Dが真に増加する経路しか作られず、
// 循環が発生しない(=必ず極大点に収束する)ことが保証される。
// JFAは近似アルゴリズムのためDにわずかな誤差(サブピクセル〜数ピクセル程度)が乗ることがあり、
// 閾値なしで「わずかでも大きければ更新」してしまうと、その誤差だけで隣り合うピクセルの
// 向かう方向が入れ替わってしまい、結果としてブロック状のノイズが見える原因になる。
// そのためEPSILON以上明確に大きい場合のみ更新することでノイズに対して頑健にする。
const EPSILON: f32 = 1.0;

fn edge_dist(coord: vec2<i32>, dims: vec2<i32>) -> f32 {
    let dx = min(coord.x, dims.x - 1 - coord.x);
    let dy = min(coord.y, dims.y - 1 - coord.y);
    return f32(min(dx, dy));
}

fn compute_d(coord: vec2<i32>, dims: vec2<i32>, seed: vec2<f32>) -> f32 {
    let seed_dist = distance(vec2<f32>(coord), seed);
    return min(seed_dist, edge_dist(coord, dims));
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let self_seed = textureLoad(tex, coord, 0).xy;
    var best_d = compute_d(coord, dims, self_seed);
    var best_coord = coord;

    for (var oy = -1; oy <= 1; oy++) {
        for (var ox = -1; ox <= 1; ox++) {
            if (ox == 0 && oy == 0) {
                continue;
            }
            let ncoord = coord + vec2<i32>(ox, oy);
            if (ncoord.x < 0 || ncoord.x >= dims.x || ncoord.y < 0 || ncoord.y >= dims.y) {
                continue;
            }
            let nseed = textureLoad(tex, ncoord, 0).xy;
            let nd = compute_d(ncoord, dims, nseed);
            if (nd > best_d + EPSILON) {
                best_d = nd;
                best_coord = ncoord;
            }
        }
    }

    textureStore(outputTex, coord, vec4<f32>(f32(best_coord.x), f32(best_coord.y), best_d, 0.0));
}
