enable wgpu_binding_array;

struct LightCompositeParams {
    // 後光係数 A / 4096.0
    a: f32,
    // 光色をY=1.0で復元したRGB(CPU側で事前計算。ガモット外に出ることがあるので
    // クランプしない)
    halo_r: f32,
    halo_g: f32,
    halo_b: f32,
    // 光色のBT.601輝度(0..1)
    light_y: f32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: LightCompositeParams;

// 後光パスのエンコードと最終合成を1パスに融合したもの(exedit-inspect light
// README §4・§5)。inputTex[0]は拡張済み・陰影適用後オブジェクト(base、src/前面)、
// inputTex[1]は拡張キャンバス上の2Dボックス平均アルファ(.aだけ使う)。
//
// 後光は「輝度をアルファへ移すアンプリマルチプライ」形式でエンコードする
// (README §4: Y=4096固定、Cb/Cr=光色、a=光色Y*scale/4096)。ここではYCbCrを
// 経由せず、光色をY=1.0で復元した固定RGB(halo_r/g/b)にscale*light_yをアルファ
// として載せるだけの等価な形にしてある。
//
// 合成はglint.wglのmode==1(後方に合成/通常合成)と同じ式: base(src/前面)を
// halo(dst/背面)の上に source-over で重ねる。halo_rgbはガモット外に出ることが
// あるが、プリマルチプライドで合成して割り戻すだけなのでクランプしない
// (RGBへのクリップは実機と同じく表示側に任せる)。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let base_tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(base_tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let base = textureLoad(base_tex, coord, 0);
    let avg2d_a = textureLoad(inputTex[1], coord, 0).a;

    let scale = min(1.0, avg2d_a * params.a);
    let halo_alpha = params.light_y * scale;
    let halo_rgb = vec3<f32>(params.halo_r, params.halo_g, params.halo_b);

    let out_alpha = base.a + halo_alpha * (1.0 - base.a);
    let premul = base.rgb * base.a + halo_rgb * halo_alpha * (1.0 - base.a);

    var out_rgb = vec3<f32>(0.0);
    if (out_alpha > 1e-6) {
        out_rgb = premul / out_alpha;
    }

    textureStore(outputTex, coord, vec4<f32>(out_rgb, clamp(out_alpha, 0.0, 1.0)));
}
