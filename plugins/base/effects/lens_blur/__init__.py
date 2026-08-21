import struct

import aperio_plugin
from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ...common.params import clamp, make_generator_information, pack_expand_params
from ...common.shader_loader import compose_common_shader, effect_dirs, lib_module, shared_shader


class LensBlurEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.lens_blur_effect"
        self.display_name = "レンズブラー"
        self.description = "Blurs the input frame with a uniform-weight disc kernel, giving point lights a circular bokeh shape."

        current_dir, common_dir = effect_dirs(__file__)
        color_module = lib_module(common_dir, "color")
        math_module = lib_module(common_dir, "math")

        rgba32float = gpu_util.WrappedImagePixelFormat.Rgba32Float
        rgba16float = gpu_util.WrappedImagePixelFormat.Rgba16Float

        self.curve_shader = compose_common_shader(
            "curve", [math_module], common_dir, "curve.wgsl",
            min_output_format=rgba32float,
        )
        self.ycbcr_encode_shader = compose_common_shader(
            "ycbcr_encode", [color_module], common_dir, "ycbcr_encode.wgsl",
            min_output_format=rgba16float,
        )
        self.ycbcr_decode_shader = compose_common_shader("ycbcr_decode", [color_module], common_dir, "ycbcr_decode.wgsl")
        self.expand_shader = shared_shader("expand", common_dir, "expand.wgsl", min_output_format=rgba16float)
        self.resize_down_shader = shared_shader(
            "lens_blur_resize_down", current_dir, "resize_down.wgsl", min_output_format=rgba32float,
        )
        self.disc_blur_shader = shared_shader(
            "lens_blur_disc_blur", current_dir, "disc_blur.wgsl", min_output_format=rgba32float,
        )
        self.resize_up_shader = compose_common_shader(
            "resize_bilinear", [math_module], common_dir, "resize_bilinear.wgsl",
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
                    default_value=5,
                    suffix="px",
                    min=0,
                    max=1000,
                ),
                RequestStructureParameter.Int(
                    id="light_intensity",
                    title="光の強さ",
                    default_value=32,
                    min=0,
                    max=60,
                ),
                RequestStructureParameter.Bool(
                    id="fixed_size",
                    title="サイズ固定",
                    # exedit全体で唯一デフォルトONの効果(README §2)。
                    default_value=True,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn | None:
        args = params.args
        range_px = max(0, args.get("range", 5))
        light_intensity = args.get("light_intensity", 32)
        fixed_size = bool(args.get("fixed_size", True))

        # README §3: `範囲 = 0` は何もしない。
        if range_px == 0:
            return None

        width, height = params.width, params.height

        # README §4(3): `範囲` -> 内部半径R(0~45)への3段圧縮。各条件は直前で
        # 更新済みのRを見る(原作のネストしたif、素直な整数演算のまま移植する)。
        R = range_px
        if R > 8:
            R = R // 2 + 4
            if R > 16:
                R = R // 4 + 12
                if R > 32:
                    R = R // 8 + 28
                    if R > 45:
                        R = 45

        builder = gpu_util.PyImageGenerateBuilder()

        # (1) README §6: `サイズ固定`OFF時のみ、キャンバスをrange px均等に広げる
        # (4本の帯を透明で塗るだけ。中央は直後の配置で埋まるのでexpand.wgslの
        # fill=透明のままでよい)。最大テクスチャサイズを超える分だけ縮める。
        if fixed_size:
            gw, gh = width, height
        else:
            max_dim = aperio_plugin.image_generator.maximum_texture_size
            grow_x = clamp(range_px, 0, max(0, (max_dim - width) // 2))
            grow_y = clamp(range_px, 0, max(0, (max_dim - height) // 2))
            gw, gh = width + 2 * grow_x, height + 2 * grow_y
            expand_params = pack_expand_params(grow_x, grow_y, gw, gh)
            builder = builder.add_wgsl(self.expand_shader, expand_params, gw, gh)

        # (2) README §4「輝度カーブ」: `ぼかし`と違い`光の強さ==0`でも無条件に
        # 1往復させる(ヘルパー側が[1,100]にクランプするのでbase=1.001で回る)。
        curve_base = 1.0 + max(1, min(100, light_intensity)) * 0.001
        builder = builder.add_wgsl(self.ycbcr_encode_shader, None, gw, gh)
        builder = builder.add_wgsl(self.curve_shader, struct.pack("fii", curve_base, 0, 2), gw, gh)

        # (3)/(4) README §4(4): R/range倍に縮小(縮小率は「ぼかし半径/縮尺」を
        # 一定に保つように選ばれている ―― 元画像でのぼかし半径がちょうどrange画素に
        # なる)。カーブ空間のまま縮小する。range<=8ならR==rangeなので縮小なし。
        dw = max(1, round(R * gw / range_px))
        dh = max(1, round(R * gh / range_px))
        builder = builder.add_wgsl(
            self.resize_down_shader, struct.pack("iiii", gw, gh, dw, dh), dw, dh
        )

        # (5) README §5: 半径Rの円板カーネルで平均。
        builder = builder.add_wgsl(self.disc_blur_shader, struct.pack("iii", R, dw, dh), dw, dh)

        # (6) 輝度カーブ 逆変換 -> YCbCr復元(まだ縮小サイズのまま)。
        builder = builder.add_wgsl(self.curve_shader, struct.pack("fii", curve_base, 1, 2), dw, dh)
        builder = builder.add_wgsl(self.ycbcr_decode_shader, None, dw, dh)

        # (7) README §4(7): 元のサイズ(拡張後のキャンバス)へ線形空間で拡大。
        builder = builder.add_wgsl(self.resize_up_shader, struct.pack("ii", gw, gh), gw, gh)

        return GeneratorBuilderReturn(builder, ItemResult(gw, gh))
