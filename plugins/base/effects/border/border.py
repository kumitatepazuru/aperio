import struct

import aperio_plugin
from aperio import gpu_util
from aperio.item_structures import FileFilter, GeneratorEvent, GeneratorInformation, ItemResult, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters

from ..common.params import clamp, make_generator_information, pack_expand_params
from ..common.pattern_image import PATTERN_EXTENSIONS, PatternImageCache
from ..common.shader_loader import compose_common_shader, effect_dirs, lib_module, shared_shader


def _pack_occupancy_dir_params(radius: int, step_x: int, step_y: int, w: int, h: int, gain: float) -> bytes:
    """occupancy_dir.wgsl 用パラメータ。"""
    return struct.pack("iiiiif", radius, step_x, step_y, w, h, gain)


class BorderEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base.border_effect"
        self.display_name = "縁取り"
        self.description = "Dilates the object's alpha silhouette into a solid- or pattern-filled outline surrounding it."

        current_dir, common_dir = effect_dirs(__file__)
        blur_module = lib_module(common_dir, "blur")

        self.expand_shader = shared_shader("expand", common_dir, "expand.wgsl")
        self.occupancy_dir_shader = compose_common_shader(
            "border_occupancy_dir", [blur_module], current_dir, "occupancy_dir.wgsl"
        )
        self.encode_color_shader = shared_shader("encode_color", common_dir, "encode_color.wgsl")
        self.encode_pattern_shader = shared_shader("encode_pattern", common_dir, "encode_pattern.wgsl")
        self.tile_shader = shared_shader("tile", common_dir, "tile.wgsl")
        self.composite_shader = shared_shader("composite", common_dir, "composite.wgsl")

        self.pattern_cache = PatternImageCache("border_pattern")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, params: dict) -> GeneratorInformation:
        paths = params.get("pattern_path", [])
        if paths:
            self.pattern_cache.ensure_loaded(paths[0])

        return make_generator_information(
            self.display_name,
            [
                RequestStructureParameter.Int(
                    id="size",
                    title="サイズ",
                    default_value=3,
                    suffix="px",
                    min=0,
                    max=500,
                ),
                RequestStructureParameter.Int(
                    id="blur",
                    title="ぼかし",
                    default_value=10,
                    min=0,
                    max=100,
                ),
                RequestStructureParameter.Color(
                    id="color",
                    title="縁色の設定",
                    default_value=(0.0, 0.0, 0.0, 1.0),
                    use_alpha=False,
                ),
                RequestStructureParameter.File(
                    id="pattern_path",
                    title="パターン画像ファイル",
                    multi_selections=False,
                    open_type="file",
                    filters=[FileFilter("画像ファイル", PATTERN_EXTENSIONS)],
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn | None:
        args = params.args
        size_ui = max(0, int(clamp(args.get("size", 3), 0, 500)))

        # サイズ=0は実機と同じく即return(README §1、画像にもキャンバスにも一切触れない)。
        if size_ui == 0:
            return None

        blur_raw = max(0, int(clamp(args.get("blur", 10), 0, 100)))
        color = args.get("color", (0.0, 0.0, 0.0, 1.0))
        pattern_paths = args.get("pattern_path", [])
        pattern_path = pattern_paths[0] if pattern_paths else ""

        w, h = params.width, params.height
        max_dim = aperio_plugin.image_generator.maximum_texture_size

        # クランプ(README §2「サイズのクランプ」)。オブジェクト自身の大きさに対する
        # クランプは無く、確保済みキャンバス(max_dim)に対してだけ効く。
        size = max(0, min(size_ui, (max_dim - w) // 2, (max_dim - h) // 2))
        if size == 0:
            return None

        # gain = 1024/((カーネル幅-1)*ぼかし*0.01+1) を0..1へ正規化(README §2)。
        gain = 1.0 / (0.02 * size * blur_raw + 1.0)

        box_w, box_h = w + 2 * size, h + 2 * size

        entry = self.pattern_cache.get(pattern_path) if pattern_path else None

        # --- オブジェクトをキャンバス中央(サイズ, サイズ)へ配置(README §3.2/§3.3) ---
        object_full = gpu_util.PyImageGenerateBuilder().add_wgsl(
            self.expand_shader, pack_expand_params(size, size, box_w, box_h), box_w, box_h
        )

        # --- 被覆マップ(垂直→水平の2パス「割らないボックス和」、README §4) ---
        mask_chain = object_full
        for step_x, step_y in ((0, 1), (1, 0)):
            mask_chain = mask_chain.add_wgsl(
                self.occupancy_dir_shader,
                _pack_occupancy_dir_params(size, step_x, step_y, box_w, box_h, gain),
                box_w,
                box_h,
            )

        if entry is not None:
            loader, pattern_func = entry
            tiled_branch = (
                gpu_util.PyImageGenerateBuilder()
                .add_texture_func(pattern_func, None, loader.width, loader.height)
                .add_wgsl(self.tile_shader, struct.pack("ii", box_w, box_h), box_w, box_h)
            )
            edge_layer = (
                gpu_util.PyImageGenerateBuilder()
                .add_parallel_wgsl([mask_chain, tiled_branch])
                .add_wgsl(self.encode_pattern_shader, struct.pack("f", 1.0), box_w, box_h)
            )
        else:
            edge_layer = mask_chain.add_wgsl(
                self.encode_color_shader, struct.pack("ffff", 1.0, color[0], color[1], color[2]), box_w, box_h
            )

        # --- 最後の合成(README §5): オブジェクトを縁の上に通常合成で重ねる ---
        final_builder = (
            gpu_util.PyImageGenerateBuilder()
            .add_parallel_wgsl([object_full, edge_layer])
            .add_wgsl(self.composite_shader, None, box_w, box_h)
        )

        return GeneratorBuilderReturn(final_builder, ItemResult(box_w, box_h))
