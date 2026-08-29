import math
import struct

import aperio_plugin
from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorWgslReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information
from ...common.shader_loader import compose_common_shader, effect_dirs, lib_module


class DirectionalBlurEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.directional_blur_effect"
        self.display_name = "方向ブラー"
        self.description = "Blurs the image along a fixed direction by averaging samples on a line segment centred on each output pixel."

        current_dir, common_dir = effect_dirs(__file__)
        math_module = lib_module(common_dir, "math")

        # out_rgb はソースrgbの凸結合(重み a_i/Σa は非負・合計1)なので、入力の値域を
        # 超えて増幅しない単発パス。straightなアルファ入出力でカーブ/対数も無いため、
        # min_output_format指定は不要(docs/plugin.md基準表の「なし」に該当)。
        self.directional_blur_shader = compose_common_shader(
            "directional_blur", [math_module], current_dir, "directional_blur.wgsl",
        )

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Int(
                    id="range",
                    title="範囲",
                    default_value=20,
                    suffix="px",
                    min=0,
                    max=500,
                ),
                RequestStructureParameter.Float(
                    id="angle",
                    title="角度",
                    default_value=50.0,
                    suffix="°",
                    min=-3600.0,
                    max=3600.0,
                ),
                RequestStructureParameter.Bool(
                    id="fixed_size",
                    title="サイズ固定",
                    default_value=False,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorWgslReturn | None:
        args = params.args
        range_px = max(0, args.get("range", 20))
        angle = args.get("angle", 50.0)
        fixed_size = bool(args.get("fixed_size", False))

        # README §3: 範囲=0はfunc_procの先頭でreturn 1(何もしない)。
        if range_px == 0:
            return None

        w, h = params.width, params.height

        # README §4: (-sinθ, +cosθ)系(凸エッジ/フレームバッファと同じ列)。符号は
        # 出力に影響しない(角度と角度+180が同一絵になる、README §4末尾)ので厳密な
        # 符号一致は不要。Q16固定小数点化は原作バイナリの整数演算再現のためだけの
        # 精度欠落なので廃止し、単位ベクトルのまま浮動小数点でシェーダーへ渡す。
        theta = math.radians(angle)
        dir_x = -math.sin(theta)
        dir_y = math.cos(theta)

        # README §5: 範囲の生値を見た4分岐でサンプル歩幅stepと片側サンプル数Nを
        # 付け替える。どの枝でも N*step ≈ range(片側到達距離は常にrange画素で
        # 一定、変わるのはサンプル密度だけ)。
        if range_px < 16:
            step = 0.25
            n = range_px * 4
        elif range_px < 32:
            step = 0.5
            n = range_px * 2
        elif range_px <= 128:
            step = 1.0
            n = range_px
        else:
            step = range_px / 128
            n = 128

        # README §7(方式D、角度非依存): サイズ固定OFF時のみ四辺にrange px広げる。
        # ONなら矩形は(0,0,w,h)に戻り(キャンバス拡張なし)、シェーダー側の除数も
        # 有効サンプル数基準に切り替わる。
        if fixed_size:
            x0, y0, x1, y1 = 0, 0, w, h
        else:
            x0, y0, x1, y1 = self._grow_canvas(w, h, range_px)

        ow, oh = x1 - x0, y1 - y0

        shader_params = struct.pack(
            "iiiifffii",
            x0, y0, ow, oh,
            dir_x, dir_y, step,
            n,
            1 if fixed_size else 0,
        )

        # glint.py同様、拡張後キャンバス座標のまま元の(未拡張)ソーステクスチャを
        # オフセット付きで直接読む1発シェーダーなので、expand.wgslの別パスは不要。
        # 拡張後キャンバスの中心に対するオブジェクト中心のずれを打ち消して、
        # オブジェクトを元の位置に留める(glint.py/clip.pyと同じ式)。
        center_x = w // 2 - (x0 + ow // 2)
        center_y = h // 2 - (y0 + oh // 2)
        return GeneratorWgslReturn(
            self.directional_blur_shader, shader_params, ItemResult(ow, oh, center_x=center_x, center_y=center_y)
        )

    def _grow_canvas(self, w: int, h: int, range_px: int) -> tuple[int, int, int, int]:
        """README §7(方式D): 角度に関わらず四辺にrange px均等に広げる(単位ベクトルの
        窓なので、どの角度でもどちらか片方の軸への最大到達距離はrangeを超えない)。

        上限(最大テクスチャサイズ/フレームサイズ+オブジェクトサイズ*2)を超える間は、
        glint._grow_canvas の非対称トリム(はみ出しが大きい側から削る、放射ブラーと
        同じ)とは異なり、中心という概念が無いエフェクトなので左右・上下をそれぞれ
        対称に1pxずつ交互に削る(README §7)。"""
        x0, y0 = -range_px, -range_px
        x1, y1 = w + range_px, h + range_px

        max_dim = aperio_plugin.image_generator.maximum_texture_size
        frame_state = aperio_plugin.store_manager.get_state().frame_state
        lim_w = min(max_dim, frame_state.width + 2 * w)
        lim_h = min(max_dim, frame_state.height + 2 * h)

        trim_from_left = True
        while x1 - x0 > lim_w:
            if trim_from_left:
                x0 += 1
            else:
                x1 -= 1
            trim_from_left = not trim_from_left

        trim_from_top = True
        while y1 - y0 > lim_h:
            if trim_from_top:
                y0 += 1
            else:
                y1 -= 1
            trim_from_top = not trim_from_top

        return x0, y0, x1, y1
