import struct

import aperio_plugin
from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ..common.color import bt601_encode
from ..common.params import clamp, make_generator_information, pack_box_average_dir_params, pack_expand_params
from ..common.shader_loader import compose_common_shader, effect_dirs, lib_module, shared_shader


class LightEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base.light_effect"
        self.display_name = "ライト"
        self.description = "Adds a colored shading pass over the object and a soft halo behind it, both driven by the object's own alpha."

        current_dir, common_dir = effect_dirs(__file__)
        blur_module = lib_module(common_dir, "blur")

        rgba32float = gpu_util.WrappedImagePixelFormat.Rgba32Float

        self.box_average_dir_shader = compose_common_shader(
            "light_box_average_dir", [blur_module], common_dir, "box_average_dir.wgsl"
        )
        self.expand_shader = shared_shader("expand", common_dir, "expand.wgsl")
        self.invert_alpha_shader = shared_shader("light_invert_alpha", current_dir, "invert_alpha.wgsl")
        self.shadow_apply_shader = shared_shader(
            "light_shadow_apply", current_dir, "shadow_apply.wgsl", output_format=rgba32float
        )
        self.composite_shader = shared_shader(
            "light_composite", current_dir, "composite.wgsl", output_format=rgba32float
        )

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, _: dict) -> GeneratorInformation:
        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Float(
                    id="strength",
                    title="強さ",
                    default_value=100.0,
                    min=0.0,
                    max=300.0,
                ),
                RequestStructureParameter.Float(
                    id="diffusion",
                    title="拡散",
                    default_value=25.0,
                    suffix="px",
                    min=0.0,
                    max=100.0,
                ),
                RequestStructureParameter.Float(
                    id="ratio",
                    title="比率",
                    default_value=0.0,
                    min=-100.0,
                    max=100.0,
                ),
                RequestStructureParameter.Bool(
                    id="backlight",
                    title="逆光",
                    default_value=False,
                ),
                RequestStructureParameter.Color(
                    id="color",
                    title="色の設定",
                    default_value=(1.0, 1.0, 1.0, 1.0),
                    use_alpha=False,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn | None:
        args = params.args
        strength_ui = clamp(args.get("strength", 100.0), 0.0, 300.0)
        diffusion_ui = clamp(args.get("diffusion", 25.0), 0.0, 100.0)
        ratio_ui = clamp(args.get("ratio", 0.0), -100.0, 100.0)
        backlight = bool(args.get("backlight", False))
        color = args.get("color", (1.0, 1.0, 1.0, 1.0))

        # 強さ0は実機と同じく即return(README §2、画像にもキャンバスにも一切触れない)。
        strength_raw = round(strength_ui * 10)
        if strength_raw == 0:
            return None

        # `比率`が`強さ`を陰影用係数Bと後光用係数Aへ分配する(README §2)。
        S = strength_raw * 4096 // 1000
        ratio_raw = round(ratio_ui * 10)
        if ratio_raw >= 0:
            A = S
            B = S if ratio_raw == 0 else (S * (1000 - ratio_raw)) // 1000
        else:
            A = (S * (1000 + ratio_raw)) // 1000
            B = S

        w, h = params.width, params.height
        max_dim = aperio_plugin.image_generator.maximum_texture_size

        # クランプ#1(陰影パスが使う半径。README §2)。
        r = max(0, round(diffusion_ui))
        if 2 * r + 1 >= w:
            r = w // 2 - 2
        if 2 * r + 1 >= h:
            r = h // 2 - 2
        r_shadow = max(0, r)
        # 実機の「確保済み最大キャンバス」制約に相当する安全策(通常は発火しない)。
        r_shadow = max(0, min(r_shadow, (max_dim - w) // 2, (max_dim - h) // 2))

        original_branch = gpu_util.PyImageGenerateBuilder()

        if B > 0:
            avg_branch = gpu_util.PyImageGenerateBuilder()
            if backlight:
                avg_branch = avg_branch.add_wgsl(self.invert_alpha_shader, None, w, h)
            avg_branch = (
                avg_branch.add_wgsl(self.box_average_dir_shader, pack_box_average_dir_params(r_shadow, 1, 0, w, h), w, h)
                .add_wgsl(self.box_average_dir_shader, pack_box_average_dir_params(r_shadow, 0, 1, w, h), w, h)
            )
            shadow_params = struct.pack("iffff", 1 if backlight else 0, B / 4096.0, color[0], color[1], color[2])
            shadowed = gpu_util.PyImageGenerateBuilder().add_parallel_wgsl([original_branch, avg_branch]).add_wgsl(
                self.shadow_apply_shader, shadow_params, w, h
            )
        else:
            # B<=0(比率=+100%)ではこのパス自体が丸ごとスキップされる(README §3)。
            shadowed = original_branch

        if A <= 0:
            # 比率=-100%: 後光パス・キャンバス拡張・最終合成を丸ごとスキップして
            # 即returnする(README §5)。
            return GeneratorBuilderReturn(shadowed, ItemResult(w, h))

        # クランプ#2: 後光パス専用に半径を上書きする。常にこちらのrを使うため、
        # 陰影パスより半径が小さくなることがある(README §2)。
        r_halo = max(0, min(r_shadow, (max_dim - w) // 2, (max_dim - h) // 2))

        nw, nh = w + 2 * r_halo, h + 2 * r_halo
        expand_params = pack_expand_params(r_halo, r_halo, nw, nh)

        base_branch = shadowed.add_wgsl(self.expand_shader, expand_params, nw, nh)
        avg2d_branch = (
            gpu_util.PyImageGenerateBuilder()
            .add_wgsl(self.expand_shader, expand_params, nw, nh)
            .add_wgsl(self.box_average_dir_shader, pack_box_average_dir_params(r_halo, 1, 0, nw, nh), nw, nh)
            .add_wgsl(self.box_average_dir_shader, pack_box_average_dir_params(r_halo, 0, 1, nw, nh), nw, nh)
        )

        # 光色をY=1.0で復元した固定RGB(後光のストレート色)と、光色自身の輝度
        # (後光の不透明度上限、README §4)をCPU側で事前計算する。
        cb, cr, y = bt601_encode(color[0], color[1], color[2])
        halo_r = 1.0 + 1.402 * cr
        halo_g = 1.0 - 0.344136 * cb - 0.714136 * cr
        halo_b = 1.0 + 1.772 * cb

        composite_params = struct.pack("fffff", A / 4096.0, halo_r, halo_g, halo_b, y)
        final_builder = gpu_util.PyImageGenerateBuilder().add_parallel_wgsl([base_branch, avg2d_branch]).add_wgsl(
            self.composite_shader, composite_params, nw, nh
        )

        return GeneratorBuilderReturn(final_builder, ItemResult(nw, nh))
