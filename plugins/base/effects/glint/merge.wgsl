struct MergeParams {
    // 0 = 前方に合成(加算)、1 = 後方に合成(通常)
    blend_mode: i32,
};

@group(0) @binding(0) var inputTex: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var outputTex: texture_storage_2d<rgba32float, write>;

@group(1) @binding(0) var<storage, read> params: MergeParams;

// inputTex[0]: 元オブジェクトを拡張キャンバスへ配置したもの(common/expand.wgsl通過済み)
// inputTex[1]: 光のバッファ(glint.wgslの出力。既にアンプリマルチプライ済み)
//
// 実機は光のバッファの上に元オブジェクトを exedit 内部の矩形転送 table[0x44]
// (0x10081b40)で描き直す(README手順8)。この関数はモード語の**上位バイト**を
// ブレンド関数テーブル 0x1009fbb0 のインデックスとして使うので、`前方`/`後方` は
// 描画順の違いではなく**合成式そのものの違い**になる:
//
//   前方に合成 = 0x11000003 -> index 0x11 -> 0x1000ae90 = 加算合成
//   後方に合成 = 0x00000003 -> index 0x00 -> 0x10007df0 = 通常合成(src over dst)
//
// 加算側はプリマルチプライドの単純加算で、アルファだけが和集合になる。元画像に
// (1 - 光のα) の減衰が掛からないのがポイントで、実機で元オブジェクトがはっきり
// 残るのはこのため。実機側にクランプは無い(両者不透明なら素の add word ptr)。
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let base_tex = inputTex[0];
    let dims = vec2<i32>(textureDimensions(base_tex));
    let coord = vec2<i32>(global_id.xy);

    if (coord.x >= dims.x || coord.y >= dims.y) {
        return;
    }

    let base = textureLoad(base_tex, coord, 0);
    let light = textureLoad(inputTex[1], coord, 0);

    var out_alpha = 0.0;
    var premul = vec3<f32>(0.0);
    if (params.blend_mode == 0) {
        // 前方に合成 ― 加算(0x1000ae90)
        out_alpha = base.a + light.a - base.a * light.a;
        premul = base.rgb * base.a + light.rgb * light.a;
    } else {
        // 後方に合成 ― 元オブジェクトを光の上へ over(0x10007df0)
        out_alpha = base.a + light.a * (1.0 - base.a);
        premul = base.rgb * base.a + light.rgb * light.a * (1.0 - base.a);
    }

    // 光のストレート色はガモット外に出ていることがある(glint.wgsl 末尾のコメント)。
    // ここでクランプすると掛け戻しが効かなくなるので、プリマルチプライドで合成して
    // 割り戻すだけにする。RGBへのクリップは実機と同じく最後の表示側に任せる。
    var out_rgb = vec3<f32>(0.0);
    if (out_alpha > 1e-6) {
        out_rgb = premul / out_alpha;
    }

    textureStore(outputTex, coord, vec4<f32>(out_rgb, clamp(out_alpha, 0.0, 1.0)));
}
