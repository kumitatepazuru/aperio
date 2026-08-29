import struct

from aperio import gpu_util
from aperio.item_structures import FileFilter, GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ..common.params import make_generator_information, pack_expand_params
from ..common.pattern_image import PATTERN_EXTENSIONS, PatternImageCache
from ..common.shader_loader import compose_common_shader, effect_dirs, lib_module, shared_shader

_MEDIA_MODE_INDEX = {"overwrite_color": 0, "luma_overwrite": 1, "luma_multiply": 2}


class CompositeImageEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.composite_image_effect"
        self.display_name = "画像ファイル合成"
        self.description = "Loads a still image file and composites it onto the object below."

        _, common_dir = effect_dirs(__file__)
        color_module = lib_module(common_dir, "color")
        math_module = lib_module(common_dir, "math")

        self.expand_shader = shared_shader("expand", common_dir, "expand.wgsl")
        self.resize_bilinear_shader = compose_common_shader(
            "resize_bilinear", [math_module], common_dir, "resize_bilinear.wgsl"
        )
        self.tile_shader = shared_shader("tile", common_dir, "tile.wgsl")
        self.composite_shader = shared_shader("composite", common_dir, "composite.wgsl")
        self.media_composite_mode_shader = compose_common_shader(
            "media_composite_mode", [color_module], common_dir, "media_composite_mode.wgsl"
        )

        self.pattern_cache = PatternImageCache("composite_image_pattern")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, params: dict) -> GeneratorInformation:
        paths = params.get("image_path", [])
        if paths:
            self.pattern_cache.ensure_loaded(paths[0])

        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.File(
                    id="image_path",
                    title="参照ファイル",
                    multi_selections=False,
                    open_type="file",
                    filters=[FileFilter("画像ファイル", PATTERN_EXTENSIONS)],
                ),
                RequestStructureParameter.Int(id="x", title="X", default_value=0, suffix="px"),
                RequestStructureParameter.Int(id="y", title="Y", default_value=0, suffix="px"),
                RequestStructureParameter.Float(
                    id="scale", title="拡大率", default_value=100.0, suffix="%", min=0.0, max=800.0
                ),
                RequestStructureParameter.Bool(id="tile_image", title="ループ画像", default_value=False),
                RequestStructureParameter.List(
                    id="mode",
                    title="合成モード",
                    values={
                        "front": "前方から合成",
                        "back": "後方から合成",
                        "overwrite_color": "色情報を上書き",
                        "luma_overwrite": "輝度をアルファ値として上書き",
                        "luma_multiply": "輝度をアルファ値として乗算",
                    },
                    default_value="front",
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn | None:
        args = params.args
        paths = args.get("image_path", [])
        path = paths[0] if paths else ""
        entry = self.pattern_cache.get(path) if path else None
        if entry is None:
            return None
        loader, image_func = entry

        x = int(args.get("x", 0))
        y = int(args.get("y", 0))
        scale_pct = max(0.0, float(args.get("scale", 100.0)))
        tile_image = bool(args.get("tile_image", False))
        mode = args.get("mode", "front")

        w, h = params.width, params.height
        scale = scale_pct / 100.0
        target_w = max(1, round(loader.width * scale))
        target_h = max(1, round(loader.height * scale))

        media_branch = gpu_util.PyImageGenerateBuilder().add_texture_func(image_func, None, loader.width, loader.height)
        if target_w != loader.width or target_h != loader.height:
            media_branch = media_branch.add_wgsl(
                self.resize_bilinear_shader, struct.pack("ii", target_w, target_h), target_w, target_h
            )

        if tile_image:
            media_branch = media_branch.add_wgsl(self.tile_shader, struct.pack("iiii", x, y, w, h), w, h)
        else:
            media_branch = media_branch.add_wgsl(
                self.expand_shader, pack_expand_params(x, y, w, h), w, h
            )

        dst_branch = gpu_util.PyImageGenerateBuilder()

        if mode in ("front", "back"):
            branches = [media_branch, dst_branch] if mode == "front" else [dst_branch, media_branch]
            builder = (
                gpu_util.PyImageGenerateBuilder()
                .add_parallel_wgsl(branches)
                .add_wgsl(self.composite_shader, None, w, h)
            )
        else:
            mode_index = _MEDIA_MODE_INDEX.get(mode, 0)
            builder = (
                gpu_util.PyImageGenerateBuilder()
                .add_parallel_wgsl([media_branch, dst_branch])
                .add_wgsl(self.media_composite_mode_shader, struct.pack("i", mode_index), w, h)
            )

        return GeneratorBuilderReturn(builder, ItemResult(w, h))
