enable wgpu_binding_array;

#import aperio::color::{bt601_encode, bt601_decode}

struct NoiseParams {
    // 0=Type1 .. 5=Type6
    noise_type: i32,
    // 0=アルファ値と乗算, 1=輝度と乗算
    mode: i32,
    // exedit-inspect noise README §3: `周期`は「セルの大きさ」ではなく密度倍率
    // (画素あたりに進む格子セル数)。大きいほど模様が細かく/小さくなる。
    density_x: f32,
    density_y: f32,
    // 経過フレーム数×速度で事前積分済みのノイズ空間オフセット(x/y はセル単位、
    // zは時間発展量。README §9: 積分は経過時間ではなく経過フレーム数に対して行う)。
    // `シード`によるオフセット(exedit-inspect noise README §4)もPython側で
    // ここに織り込み済み。
    ox: f32,
    oy: f32,
    oz: f32,
    // 0..2.0 (0%..200%)
    strength: f32,
    // 0..1.0 (0%..100%)
    threshold: f32,
    out_width: i32,
    out_height: i32,
    // exedit-inspect noise README §4: func_initが作る256x256の固定乱数場
    // ([-2048,2048]に正規化済み)。種が固定なのでプロジェクトに依らず常に同じ値。
    field: array<f32, 65536>,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: ImageStorageTexture;

@group(1) @binding(0) var<storage, read> params: NoiseParams;

const PI: f32 = 3.14159265358979;

fn cos_interp(a: f32, b: f32, t: f32) -> f32 {
    let w = 0.5 - 0.5 * cos(PI * t);
    return mix(a, b, w);
}

fn wrap256(v: i32) -> i32 {
    return ((v % 256) + 256) % 256;
}

// exedit-inspect noise README §6: 3D化は「zが1増えるごとに、同じ256x256の場を
// x/yともに79格子分ずらした窓を見る」という安上がりな方式。
fn field_at(cell_x: i32, cell_y: i32, z_slice: i32) -> f32 {
    let off = 79 * z_slice;
    let col = wrap256(cell_x + off);
    let row = wrap256(cell_y + off);
    return params.field[row * 256 + col];
}

// exedit-inspect noise README §6: x/zはコサイン補間、yは線形補間(非対称)。8点の格子値から
// 3D値ノイズを1サンプル取り、[-1,1]へ正規化して返す。
fn value_noise3(pos: vec3<f32>) -> f32 {
    let base = floor(pos);
    let frac = pos - base;
    let ix = i32(base.x);
    let iy = i32(base.y);
    let iz = i32(base.z);

    let c000 = field_at(ix, iy, iz);
    let c100 = field_at(ix + 1, iy, iz);
    let c010 = field_at(ix, iy + 1, iz);
    let c110 = field_at(ix + 1, iy + 1, iz);
    let c001 = field_at(ix, iy, iz + 1);
    let c101 = field_at(ix + 1, iy, iz + 1);
    let c011 = field_at(ix, iy + 1, iz + 1);
    let c111 = field_at(ix + 1, iy + 1, iz + 1);

    let x00 = cos_interp(c000, c100, frac.x);
    let x10 = cos_interp(c010, c110, frac.x);
    let x01 = cos_interp(c001, c101, frac.x);
    let x11 = cos_interp(c011, c111, frac.x);

    let y0 = mix(x00, x10, frac.y);
    let y1 = mix(x01, x11, frac.y);

    return cos_interp(y0, y1, frac.z) / 2048.0;
}

// 6オクターブfBm
fn fbm_signed(pos: vec3<f32>) -> f32 {
    var p = pos;
    var acc = 0.0;
    for (var i = 0u; i < 6u; i++) {
        acc = acc * 0.5 + value_noise3(p);
        p = p * 0.5 + vec3<f32>(7.0, 17.0, 31.0);
    }
    return acc;
}

fn fbm_abs(pos: vec3<f32>) -> f32 {
    var p = pos;
    var acc = 0.0;
    for (var i = 0u; i < 6u; i++) {
        acc = acc * 0.5 + abs(value_noise3(p));
        p = p * 0.5 + vec3<f32>(7.0, 17.0, 31.0);
    }
    return acc;
}

// README記載の6タイプの合成式。戻り値は[0,1]。
fn noise_value(pos: vec3<f32>, noise_type: i32) -> f32 {
    if (noise_type == 1) {
        // Type2: 0x1000 - 2*fBm(|.|)  →  1 - fbm_abs
        return clamp(1.0 - fbm_abs(pos), 0.0, 1.0);
    } else if (noise_type == 2) {
        // Type3: 2*fBm(|.|)  →  fbm_abs
        return clamp(fbm_abs(pos), 0.0, 1.0);
    } else if (noise_type == 3) {
        // Type4(標本器B, 0x1006cee0)は中身が未読(README §8/§10)なので、
        // 同じ標本器Aを別スケール・別オフセットで流用する近似。この型だけは
        // オリジナルと見た目が完全には一致しない。
        let shifted = pos * 4.0 + vec3<f32>(777.0, 421.0, 233.0);
        return clamp(value_noise3(shifted) * 0.5 + 0.5, 0.0, 1.0);
    } else if (noise_type == 4) {
        // Type5: 符号ありではなく絶対値版のfBmを使う
        return clamp(0.5 + 0.5 * sin(2.0 * PI * (pos.x / 128.0 - fbm_abs(pos))), 0.0, 1.0);
    } else if (noise_type == 5) {
        // Type6: 符号あり版のfBmを使う
        return clamp(0.5 + 0.5 * sin(2.0 * PI * sin(4.0 * PI * fbm_signed(pos))), 0.0, 1.0);
    }
    // Type1: fBm(符号つき) + 0x800  →  0.5*fbm_signed + 0.5
    return clamp(fbm_signed(pos) * 0.5 + 0.5, 0.0, 1.0);
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let coord = vec2<i32>(global_id.xy);
    if (coord.x >= params.out_width || coord.y >= params.out_height) {
        return;
    }

    let src = textureLoad(inputTex[0], coord, 0);

    let pos = vec3<f32>(
        f32(coord.x) * params.density_x + params.ox,
        f32(coord.y) * params.density_y + params.oy,
        params.oz,
    );
    let v01 = noise_value(pos, params.noise_type);

    // しきい値: 下側を切って上側へ伸ばすコントラストストレッチ。
    let denom = max(1.0 - params.threshold, 1e-4);
    let v = clamp((v01 - params.threshold) / denom, 0.0, 1.0);

    // 強さの2レジーム式(README): s<=1.0は目標値への線形補間、s>1.0は減衰。
    let s = params.strength;
    var mult_factor: f32;
    if (s <= 1.0) {
        mult_factor = 1.0 + (v - 1.0) * s;
    } else {
        mult_factor = v * (2.0 - s);
    }

    var out_rgb = src.rgb;
    var out_a = src.a;
    if (params.mode == 1) {
        // 輝度と乗算: PIXEL_YCAのYだけを乗算し、Cb/Crは変更しない
        // (アルファ値と乗算とは別のフィールドを触る、README §9)。
        let ycc = bt601_encode(src.rgb);
        let y = clamp(ycc.z * mult_factor, 0.0, 1.0);
        out_rgb = bt601_decode(ycc.x, ycc.y, y);
    } else {
        out_a = clamp(src.a * mult_factor, 0.0, 1.0);
    }

    textureStore(outputTex, coord, vec4<f32>(out_rgb, out_a));
}
