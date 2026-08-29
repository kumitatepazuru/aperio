import math
import struct

import aperio_plugin
from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, GeneratorWgslReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import make_generator_information
from ...common.shader_loader import effect_dirs, shared_shader


def ftol(value: float) -> int:
    """CRT `_ftol` ―― 0方向への切り捨て。"""
    return math.trunc(value)


def scale_axis(size: int, ratio: float) -> int:
    """1軸ぶんの倍率適用。実機は掛けるたびに `+0.5` してから `_ftol` する
    (`拡大率` フィルタ本体の0方向切り捨てとは丸め方が違う)。"""
    return ftol(size * ratio + 0.5)


class ResizeEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "basic_effect.resize"
        self.display_name = "リサイズ"
        self.description = "Resizes the image by resampling pixels."

        current_dir, _ = effect_dirs(__file__)
        self.resize_shader = shared_shader("base_effect_resize", current_dir, "resize.wgsl")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Float(
                    id="zoom",
                    title="拡大率",
                    default_value=100.0,
                    suffix="％",
                    min=0.0,
                ),
                RequestStructureParameter.Float(
                    id="zoom_x",
                    title="X",
                    default_value=100.0,
                    suffix="％",
                    min=0.0,
                ),
                RequestStructureParameter.Float(
                    id="zoom_y",
                    title="Y",
                    default_value=100.0,
                    suffix="％",
                    min=0.0,
                ),
                RequestStructureParameter.Bool(
                    id="no_interpolation",
                    title="補間なし",
                    default_value=False,
                ),
                RequestStructureParameter.Bool(
                    id="pixel_size",
                    title="ドット数でサイズ指定",
                    default_value=False,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorWgslReturn | GeneratorBuilderReturn:
        args = params.args
        pixel_size = bool(args.get("pixel_size", False))
        no_interpolation = bool(args.get("no_interpolation", False))

        # wgpu上の確保済み最大キャンバスへのクランプ。テクスチャ最大辺長は縦横で
        # 同じなので max_w/max_h は同値だが、実機の比較式の形を残しておく。
        max_w = max_h = aperio_plugin.image_generator.maximum_texture_size

        if pixel_size:
            # `ドット数でサイズ指定`: X/Y は百分率ではなくそのまま目標ピクセル数。
            # `拡大率` は使わない(実機も比率モードで作った値をこの値で上書きする)。
            new_width = math.trunc(float(args.get("zoom_x", 100.0)))
            new_height = math.trunc(float(args.get("zoom_y", 100.0)))

            new_width = min(new_width, max_w)
            new_height = min(new_height, max_h)
        else:
            zoom = float(args.get("zoom", 100.0)) / 100.0
            zoom_x = float(args.get("zoom_x", 100.0)) / 100.0
            zoom_y = float(args.get("zoom_y", 100.0)) / 100.0
            new_width = scale_axis(scale_axis(params.width, zoom), zoom_x)
            new_height = scale_axis(scale_axis(params.height, zoom), zoom_y)
            # 比率モードは先に頭打ちになる軸を最大値で固定し、もう一方を
            # 新しい目標アスペクト比で再計算する(アスペクト比を保つ)
            if max_h * params.width <= max_w * params.height:
                if new_width > max_w:
                    new_height = math.trunc(max_w * new_height / new_width)
                    new_width = max_w
            elif new_height > max_h:
                new_width = math.trunc(max_h * new_width / new_height)
                new_height = max_h

        # 実機は新しい幅・高さのどちらかが0以下ならそのオブジェクトをこの回の描画から外す。
        # TODO: アイテムごとに描画をスキップする方法を作る
        new_width = max(1, new_width)
        new_height = max(1, new_height)

        if new_width == params.width and new_height == params.height:
            return GeneratorBuilderReturn(gpu_util.PyImageGenerateBuilder(), ItemResult(params.width, params.height))

        shader_params = struct.pack("iiii", new_width, new_height, 1 if no_interpolation else 0, 0)

        return GeneratorWgslReturn(
            self.resize_shader,
            shader_params,
            ItemResult(new_width, new_height),
        )
