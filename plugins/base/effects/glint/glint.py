import math
import os
import struct

import aperio_plugin
from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio.gpu_util import PyCompiledWgsl
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

# 光線の到達距離。中心から距離dのソース画素は D = 4d まで光を伸ばすので、
# キャンバスは光条の端がちょうど収まる大きさに広げられる(README手順3)。
_RAY_REACH = 4

# 「高速化」ON時の maxd(=サンプル数上限Rの元になる量)の頭打ち。実機にも同じ場所に
# 効く draft フラグ(fpip->flag & 0x200)があり、そちらは 50 で頭打ちにする。
# ただし50だと数百px級のオブジェクトでも遠方の光条が露骨に破線になるため、ここでは
# 256(→ R=192)を採る。maxd <= 256、おおむね512px角までのオブジェクトでは
# 「高速化」OFFと完全に同一の出力になり、それより大きい場合だけコストが頭打ちになる。
# Rはサンプル数(=サンプル間隔)の上限であって光線の到達距離ではないので、
# この値を変えてもキャンバスサイズも光条の長さも変わらない(README手順4)。
_FAST_MAXD_CAP = 256


class GlintEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base.glint_effect"
        self.display_name = "閃光"
        self.description = "Casts radial light streaks from a centre point by averaging the source along each pixel's ray."

        current_dir = os.path.dirname(__file__)
        common_dir = os.path.join(current_dir, "..", "common")
        color_module = gpu_util.create_composable_module(os.path.join(common_dir, "lib", "color.wgsl"))

        self.glint_shader = PyCompiledWgsl.compose_new(
            "glint",
            [color_module],
            gpu_util.create_naga_module(os.path.join(current_dir, "glint.wgsl")),
            aperio_plugin.image_generator,
        )

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return GeneratorInformation(
            display_name=self.display_name,
            duration_frames=None,
            max_frame=None,
            min_frame=None,
            structure=[
                RequestStructureParameter.Int(
                    id="strength",
                    title="強さ",
                    default_value=100,
                    min=0,
                    max=100,
                ),
                # 実機のX/Yトラックバー。単位は画素で、UIの数値がそのまま生値になる。
                # 実機側でもグループ化された多成分コントロールなのでVec2Intにまとめる。
                RequestStructureParameter.Vec2Int(
                    id="center",
                    title="中心",
                    default_value=(0, 0),
                    suffix="px",
                    min=-2000,
                    max=2000,
                ),
                RequestStructureParameter.Color(
                    id="color",
                    title="光色",
                    default_value=(1.0, 1.0, 1.0, 1.0),
                    use_alpha=False,
                ),
                # 実機のex_data bit24セット(=「指定無し (元画像の色)」)が工場出荷既定。
                # オンだと各画素自身の色で光るので、赤いハイライトは赤い光条になる。
                # オフ(光色指定)だとソースの輝度を一切読まず、被覆率だけを測って
                # 指定色に掛ける。暗い光色ほど`強さ`の下限が上がる(README手順7)。
                RequestStructureParameter.Bool(
                    id="use_source_color",
                    title="光色: 元画像の色を使う",
                    default_value=True,
                ),
                RequestStructureParameter.List(
                    id="blend_mode",
                    title="合成",
                    values={
                        "front": "前方に合成",
                        "back": "後方に合成",
                        "light_only": "光成分のみ",
                    },
                    default_value="front",
                ),
                RequestStructureParameter.Bool(
                    id="fixed_size",
                    title="サイズ固定",
                    default_value=False,
                ),
                # ONにすると遠方のサンプル間隔だけを粗くして高速化する。
                # 512px角程度までのオブジェクトでは出力が完全に一致する(_FAST_MAXD_CAP)。
                RequestStructureParameter.Bool(
                    id="fast_mode",
                    title="高速化",
                    default_value=True,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn | None:
        args = params.args
        strength_ui = max(0, min(100, args.get("strength", 100)))
        center = args.get("center", (0, 0))
        track_x = max(-2000, min(2000, int(center[0])))
        track_y = max(-2000, min(2000, int(center[1])))
        color = args.get("color", (1.0, 1.0, 1.0, 1.0))
        use_source_color = bool(args.get("use_source_color", True))
        blend_mode: str = args.get("blend_mode", "front")
        fixed_size = bool(args.get("fixed_size", False))
        fast_mode = bool(args.get("fast_mode", True))

        # 強さ0は即return。実機も画像にもキャンバスにも一切触れない。
        if strength_ui == 0:
            return None

        w, h = params.width, params.height

        # 強さは輝度への掛け算ではなく**しきい値の引き算**。実機は
        # strength = trunc(raw*4096/1000)、T = max(0, 4096-strength) で、
        # 4096で割って正規化するとUI値/100がそのまま強さになる。
        # キャンバスの拡張量にだけは掛け算として効くので、そちらは実機と同じ
        # 固定小数(4096)のまま扱う。
        strength_fixed = strength_ui * 4096 // 100
        threshold = max(0.0, 1.0 - strength_ui / 100.0)

        cx = w // 2 + track_x
        cy = h // 2 + track_y

        # サンプル数上限R。0.75*maxdのつもりの式だが、切り捨てが2回別々に起きるので
        # trunc(0.75*maxd)とは一致しない(maxd%4==3のとき常に1小さい)。
        maxd = max(abs(cx), abs(cy), abs(cx - w), abs(cy - h))
        maxd = min(maxd, int(math.sqrt(w * w + h * h)))  # 画像対角線でクランプ
        if fast_mode:
            maxd = min(maxd, _FAST_MAXD_CAP)
        sample_cap = maxd // 4 + maxd // 2

        # サイズ固定は拡張結果を全部捨てて元の矩形に戻す。光線の計算自体は変わらない
        # ので、光条は同じように伸びようとしてオブジェクトの矩形で切られる。
        if fixed_size:
            x0, y0, x1, y1 = 0, 0, w, h
        else:
            x0, y0, x1, y1 = self._grow_canvas(w, h, cx, cy, strength_fixed)

        ow, oh = x1 - x0, y1 - y0

        # mode: 0=前方に合成(加算)、1=後方に合成(通常)、2=光成分のみ。
        # glint.wgsl は元オブジェクト座標(x, y)を自前で計算済みなので、旧
        # expand.wgsl の逆写像と一致するその座標からベースピクセルを直接読み、
        # 旧merge.wgslのブレンド式もそのまま内部で適用する(拡張・合成の
        # 2パスを削って1ディスパッチにまとめてある)。
        if blend_mode == "light_only":
            mode = 2
        elif blend_mode == "back":
            mode = 1
        else:
            mode = 0

        glint_params = struct.pack(
            "iiiiiiififffi",
            x0, y0, ow, oh,
            cx, cy,
            sample_cap,
            threshold,
            1 if use_source_color else 0,
            color[0], color[1], color[2],
            mode,
        )

        builder = gpu_util.PyImageGenerateBuilder().add_wgsl(self.glint_shader, glint_params, ow, oh)

        # 拡張後キャンバスの中心に対するオブジェクト中心のずれを打ち消して、
        # オブジェクトを元の位置に留める(実機のfpip+0xD4/+0xD8の補正と同じ式)。
        center_x = w // 2 - (x0 + ow // 2)
        center_y = h // 2 - (y0 + oh // 2)
        return GeneratorBuilderReturn(builder, ItemResult(ow, oh, center_x=center_x, center_y=center_y))

    def _grow_canvas(self, w: int, h: int, cx: int, cy: int, strength_fixed: int) -> tuple[int, int, int, int]:
        """光条の端がちょうど収まるまでキャンバスを広げ、上限で切り詰める(README手順3)。

        拡張量が strength 倍される点は実機どおり。強さは本来しきい値であって長さでは
        ないので幾何的な到達距離は4倍のままだが、「暗い先端はどうせしきい値を超えない
        だろう」という前提でキャンバスだけが切り詰められている。
        """
        x0 = min(cx - _RAY_REACH * cx, 0)
        y0 = min(cy - _RAY_REACH * cy, 0)
        x1 = max((cx - w) + _RAY_REACH * (w - cx), 0)
        y1 = max((cy - h) + _RAY_REACH * (h - cy), 0)

        # Pythonの>>は算術シフトなので、実機のsar(負値はfloor)と一致する。
        x0, y0 = (x0 * strength_fixed) >> 12, (y0 * strength_fixed) >> 12
        x1, y1 = ((x1 * strength_fixed) >> 12) + w, ((y1 * strength_fixed) >> 12) + h

        # 2つの上限で切り詰める。実機はオブジェクトキャンバスの最大幅・最大高と
        # 「フレームサイズ + オブジェクトサイズ*2」の小さいほうを使う。前者にあたるのは
        # aperioでは確保できるテクスチャの最大辺長。
        max_dim = aperio_plugin.image_generator.maximum_texture_size
        frame_state = aperio_plugin.store_manager.get_state().frame_state
        lim_w = min(max_dim, frame_state.width + 2 * w)
        lim_h = min(max_dim, frame_state.height + 2 * h)

        # はみ出しが大きい側を1画素ずつ削る
        while x1 - x0 > lim_w:
            if x0 < w - x1:
                x0 += 1
            else:
                x1 -= 1
        while y1 - y0 > lim_h:
            if y0 < h - y1:
                y0 += 1
            else:
                y1 -= 1

        return x0, y0, x1, y1
