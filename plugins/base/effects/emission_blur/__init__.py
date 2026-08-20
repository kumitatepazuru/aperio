import math
import struct

import aperio_plugin
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorWgslReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import clamp, make_generator_information
from ...common.shader_loader import effect_dirs, shared_shader


class EmissionBlurEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.emission_blur_effect"
        self.display_name = "放射ブラー"
        self.description = "Radially blurs the image by averaging each pixel along the segment toward a centre point, producing a zoom-like streak."

        current_dir, _ = effect_dirs(__file__)

        # out_rgbはソースrgbの凸結合(重みs.a/sum_aは非負・合計1)、out_aもn個の
        # サンプル(範囲外は0扱い、各<=1)の単純平均で常に[0,1]に収まる単発パス。
        # straightなアルファ入出力でカーブ/対数の増幅も無いため、min_output_format
        # 指定は不要(docs/plugin.md基準表の「なし」に該当。directional_blurと同型)。
        self.emission_blur_shader = shared_shader("emission_blur", current_dir, "emission_blur.wgsl")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                # 表示スケール10(実機の生値0~750はUI上0.0~75.0)。UIの数値がそのまま
                # 「中心までの距離の何%まで走るか」になる(README §3)。
                RequestStructureParameter.Float(
                    id="range",
                    title="範囲",
                    default_value=20.0,
                    suffix="%",
                    min=0.0,
                    max=75.0,
                ),
                # 実機のX/Yトラックバー。単位は画素で、UIの数値がそのまま生値になる。
                # glint.py の center と同じ扱い(実機側でもグループ化された多成分
                # コントロールなのでVec2Intにまとめる)。
                RequestStructureParameter.Vec2Int(
                    id="center",
                    title="中心",
                    default_value=(0, 0),
                    suffix="px",
                    min=-2000,
                    max=2000,
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
        range_ui = max(0.0, args.get("range", 20.0))
        center = args.get("center", (0, 0))
        track_x = clamp(int(center[0]), -2000, 2000)
        track_y = clamp(int(center[1]), -2000, 2000)
        fixed_size = bool(args.get("fixed_size", False))

        # README §3: 範囲=0はfunc_procの先頭でreturn 1(何もしない)。
        if range_ui == 0.0:
            return None

        w, h = params.width, params.height

        # README §4: cx/cy(放射の中心)。
        cx = w / 2 + track_x
        cy = h / 2 + track_y

        # R: 中心から4辺までの距離の最大を、画像対角線でクランプしたもの。
        r = max(abs(cx), abs(cy), abs(cx - w), abs(cy - h))
        r = min(r, math.sqrt(w * w + h * h))

        # range_frac = UI値/100(実機の生値/1000と同じ。README §3)。
        range_frac = range_ui / 100.0

        # R'(サンプル数の基準)。実機のQ12丸めは落として実数のまま扱う(docs/plugin.md #1)。
        r_prime = (1.0 - range_frac) * r + r / 2.0

        # README §6(方式D): サイズ固定OFF時のみ、サンプル線分が届く範囲ちょうどに
        # キャンバスを広げる。range_fracが1に近づくと分母が潰れるが、UI上限75.0
        # (range_frac<=0.75)ではまだ有限(1/(1-0.75)=4)なので実際には起きない。
        # それでも念のため防御的にクランプしておく(docs/plugin.md #5)。
        if fixed_size:
            x0, y0, x1, y1 = 0, 0, w, h
        else:
            x0, y0, x1, y1 = self._grow_canvas(w, h, cx, cy, range_frac)

        ow, oh = x1 - x0, y1 - y0

        shader_params = struct.pack(
            "iiiiffff",
            x0, y0, ow, oh,
            cx, cy,
            r_prime, range_frac,
        )

        # glint.py同様、拡張後キャンバス座標のまま元の(未拡張)ソーステクスチャを
        # オフセット付きで直接読む1発シェーダーなので、expand.wgslの別パスは不要。
        # 拡張後キャンバスの中心に対するオブジェクト中心のずれを打ち消して、
        # オブジェクトを元の位置に留める(実機のfpip+0xD4/+0xD8の補正と同じ式)。
        center_x = w // 2 - (x0 + ow // 2)
        center_y = h // 2 - (y0 + oh // 2)
        return GeneratorWgslReturn(
            self.emission_blur_shader, shader_params, ItemResult(ow, oh, center_x=center_x, center_y=center_y)
        )

    def _grow_canvas(self, w: int, h: int, cx: float, cy: float, range_frac: float) -> tuple[int, int, int, int]:
        """README §6(方式D): 出力画素xのサンプル線分はxからx+(cx-x)*range_fracまで
        走るので、x0はその終点がちょうど0に届くx、x1はちょうどwに届くx(y0/y1も同様)。

        2つの上限(最大テクスチャサイズ/フレームサイズ+オブジェクトサイズ*2)を超える
        間は、はみ出しが大きい側から1pxずつ削る(glint._grow_canvasと同じ非対称
        トリム。emission_blurは中心という概念を持つため、directional_blurの対称
        トリムではなくこちらがREADME §6の削り方と一致する)。"""
        safe_frac = min(range_frac, 0.99)
        inv = 1.0 - safe_frac

        x0 = min(cx - cx / inv, 0.0)
        x1 = w + max((w - cx) / inv - (w - cx), 0.0)
        y0 = min(cy - cy / inv, 0.0)
        y1 = h + max((h - cy) / inv - (h - cy), 0.0)

        # サンプル線分を絶対に切り詰めないよう、負側はfloor(より外側へ)、正側は
        # ceil(より外側へ)で整数化する(glint._grow_canvasと同じ丸め方針)。
        x0, y0 = math.floor(x0), math.floor(y0)
        x1, y1 = math.ceil(x1), math.ceil(y1)

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
