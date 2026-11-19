import os

import aperio_plugin
from aperio import gpu_util
from aperio_plugin.plugin_base import MainPluginBase

from ..common.shader_loader import LIB_DIR, load_text

CURRENT_DIR = os.path.dirname(os.path.abspath(__file__))

BLEND_MODES: list[tuple[str, str, int]] = [
    ("normal", "通常", 0),
    ("add", "加算", 1),
    ("subtract", "減算", 2),
    ("multiply", "乗算", 3),
    ("screen", "スクリーン", 4),
    ("overlay", "オーバーレイ", 5),
    ("lighten", "比較(明)", 6),
    ("darken", "比較(暗)", 7),
    ("luminance", "輝度", 8),
    ("chroma", "色差", 9),
    ("linear_burn", "陰影", 10),
    ("linear_light", "明暗", 11),
    ("difference", "差分", 12),
]


def _compile(id: str, source: str, mode: int) -> gpu_util.PyCompiledShader:
    """BLEND_MODE を変えて同じソースからブレンドモード1つぶんのラスターシェーダーを作る。"""
    return gpu_util.PyCompiledShader.from_slang_raster(
        # 名前はパイプラインキャッシュのキーになるので、モードごとに分ける
        f"base_blend_{id}",
        source,
        aperio_plugin.context.image_generator,
        search_paths=[CURRENT_DIR, LIB_DIR, aperio_plugin.plugin_manager.blend_shader_dir],
        defines={"BLEND_MODE": str(mode)},
        # aperio_blend.slang がレイヤーをサンプルするサンプラー(マネージャー組み込みの通常合成と同じ設定)
        sampler_options=gpu_util.PySamplerOptions("clamp_to_edge", "linear"),
        vertex_entry="vs_main",
        fragment_entry="fs_main",
        vertex_count=6,
    )


class AperioBaseBlendModePlugin(MainPluginBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_blend"
        self.display_name = "基本合成モード"
        self.description = "This plugin provides the 13 blend modes of AviUtl for Aperio."
        self.version = "1.0.0"
        self.author = "Aperio"

        source = load_text(os.path.join(CURRENT_DIR, "blend_modes.slang"))
        for id, display_name, mode in BLEND_MODES:
            aperio_plugin.plugin_manager.add_blend_mode(id, display_name, _compile(id, source, mode))
