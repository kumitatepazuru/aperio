import os
import struct

import aperio_plugin
from aperio import gpu_util
from aperio.item_structures import FileFilter, GeneratorEvent, GeneratorInformation, RequestStructureParameter
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoGenerateParameters, VideoObjectGeneratorBase

from ...common.params import aspect_shrink_factors
from ...common.pattern_image import PATTERN_EXTENSIONS, PatternImageCache
from ...common.shader_loader import effect_dirs, shared_slang_shader

# figure.slang の shape
_SHADER_CIRCLE = 0
_SHADER_RECTANGLE = 1
_SHADER_POLYGON = 2
_SHADER_STAR = 3

# 多角形の頂点数。星型はシェーダー側で10固定。
_POLYGON_VERTICES = {"triangle": 3, "pentagon": 5, "hexagon": 6}

# `shape` の組み込みの値。FIGURE_DIR の PNG の名前がこれと重なっても組み込みが優先される。
_RESERVED_SHAPES = {"background", "circle", "rectangle", "triangle", "pentagon", "hexagon", "star", "file"}

# カスタム図形のディレクトリパス
FIGURE_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "images")


def _list_figure_images() -> list[str]:
    """FIGURE_DIR 直下の PNG の名前(拡張子なし)を名前順で返す。フォルダが無ければ空。"""
    try:
        entries = os.listdir(FIGURE_DIR)
    except OSError:
        return []
    names = [
        os.path.splitext(entry)[0]
        for entry in entries
        if entry.lower().endswith(".png") and os.path.isfile(os.path.join(FIGURE_DIR, entry))
    ]
    return sorted(names, key=str.casefold)


def _figure_image_path(name: str) -> str | None:
    """`図形名` に対応する FIGURE_DIR の PNG のパス。FIGURE_DIR の外を指す名前や、無いファイルは None。"""
    if not name or os.path.basename(name) != name:
        return None
    path = os.path.join(FIGURE_DIR, f"{name}.png")
    return path if os.path.isfile(path) else None


class FigureObject(VideoObjectGeneratorBase):
    """
    図形(背景・円・四角形・三角形・五角形・六角形・星型、または画像ファイルの明るさを形にしたもの)を
    指定の色で描くオブジェクトプラグイン。
    """

    def __init__(self) -> None:
        super().__init__()

        self.name = "base_object.figure_object"
        self.display_name = "図形"
        self.description = "図形(円・四角形・多角形・星型・画像)を指定の色で描きます。"

        current_dir, common_dir = effect_dirs(__file__)
        self.figure_shader = shared_slang_shader("figure", current_dir, "figure.slang")
        self.image_mask_shader = shared_slang_shader("figure_image_mask", current_dir, "image_mask.slang")
        # 被覆率(.a)に色を載せる共通パス。シャドー・縁取り・エッジ抽出と同じもの。
        self.encode_color_shader = shared_slang_shader("encode_color", common_dir, "encode_color.slang")

        self.image_cache = PatternImageCache("figure_image")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    # TODO: あまりにも直感的ではない設計なので、将来的にこれはあくまでもaviutl互換用の隠れ実装にし、ユーザー向けへは改良したものに変更する
    def on_request_structure(self, params: dict) -> GeneratorInformation:
        paths = params.get("figure_path", [])
        if paths:
            self.image_cache.ensure_loaded(paths[0])
        folder_path = _figure_image_path(params.get("shape", ""))
        if folder_path:
            self.image_cache.ensure_loaded(folder_path)

        # 組み込みの図形名と同じ名前の PNG は、組み込みの方を優先して一覧に出さない。
        figure_images = {name: name for name in _list_figure_images() if name not in _RESERVED_SHAPES}

        return GeneratorInformation(
            display_name=self.display_name,
            duration_frames=300,
            max_frame=None,
            min_frame=None,
            structure=[
                RequestStructureParameter.Int(
                    id="size",
                    title="サイズ",
                    default_value=100,
                    suffix="px",
                    min=0,
                    max=4000,
                ),
                RequestStructureParameter.Float(
                    id="aspect",
                    title="縦横比",
                    default_value=0.0,
                    min=-100.0,
                    max=100.0,
                ),
                RequestStructureParameter.Int(
                    id="line_width",
                    title="ライン幅",
                    default_value=4000,
                    suffix="px",
                    min=0,
                    max=4000,
                ),
                RequestStructureParameter.Color(
                    id="color",
                    title="色の設定",
                    default_value=(1.0, 1.0, 1.0, 1.0),
                    use_alpha=False,
                ),
                RequestStructureParameter.List(
                    id="shape",
                    title="図形の種類",
                    values={
                        "background": "背景",
                        "circle": "円",
                        "rectangle": "四角形",
                        "triangle": "三角形",
                        "pentagon": "五角形",
                        "hexagon": "六角形",
                        "star": "星型",
                        **figure_images,
                        "file": "(ファイルから選択)",
                    },
                    default_value="circle",
                ),
                # TODO: ファイルから選択の時のみこれが表示されるようにする
                RequestStructureParameter.File(
                    id="figure_path",
                    title="画像ファイル",
                    multi_selections=False,
                    open_type="file",
                    filters=[FileFilter("画像ファイル", PATTERN_EXTENSIONS)],
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn | None:
        args = params.args
        size = round(args.get("size", 100))
        line_width = round(args.get("line_width", 4000))
        aspect = float(args.get("aspect", 0.0))
        shape = args.get("shape", "circle")
        color = args.get("color", (1.0, 1.0, 1.0, 1.0))

        # サイズ・ライン幅は 0 以下なら何も描かない。
        if size <= 0 or line_width <= 0:
            return None

        # 縦横比は短くなる側の辺だけを縮める(正なら幅、負なら高さ)。±100では必ず0になって消え、
        # この判定は寸法を使わない背景・多角形・画像の図形にも効く。
        width_scale, height_scale = aspect_shrink_factors(aspect)
        box_w = round(size * width_scale)
        box_h = round(size * height_scale)
        if box_w <= 0 or box_h <= 0:
            return None

        # 最大キャンバスで各辺を独立にクランプする(正方形が長方形になる)。
        max_dim = aperio_plugin.context.image_generator.maximum_texture_size
        box_w = min(box_w, max_dim)
        box_h = min(box_h, max_dim)

        if shape == "file":
            paths = args.get("figure_path", [])
            return self._generate_image(params, paths[0] if paths else None, size, width_scale, height_scale, color)

        # 枠になる条件はサイズ > 2*ライン幅
        hollow = int(size > 2 * line_width)

        if shape == "circle":
            return self._generate_shape(
                params, _SHADER_CIRCLE, box_w, box_h, size, line_width, 0, hollow, color
            )

        if shape == "rectangle":
            return self._generate_shape(
                params, _SHADER_RECTANGLE, box_w, box_h, size, line_width, 0, hollow, color
            )

        if shape in _POLYGON_VERTICES or shape == "star":
            # 多角形は縦横比を寸法に入れず、描画側のスケールに任せる
            side = min(size, max_dim)
            return self._generate_shape(
                params,
                _SHADER_STAR if shape == "star" else _SHADER_POLYGON,
                side,
                side,
                size,
                line_width,
                _POLYGON_VERTICES.get(shape, 0),
                0,
                color,
                scale=(width_scale, height_scale),
            )

        if shape == "background":
            # 寸法はフレームの大きさで、サイズ・縦横比は「消えるか」と「額縁になるか」の判定にしか効かない
            return self._generate_shape(
                params, _SHADER_RECTANGLE, params.width, params.height, size, line_width, 0, hollow, color
            )

        # 上のどれでもなければ FIGURE_DIR の PNG の名前。原作と同じく、見つからない・読めないときは何も描かない。
        return self._generate_image(params, _figure_image_path(shape), size, width_scale, height_scale, color)

    def _generate_shape(
        self,
        params: VideoGenerateParameters,
        shader_shape: int,
        out_w: int,
        out_h: int,
        size: int,
        line_width: int,
        vertices: int,
        hollow: int,
        color: tuple,
        scale: tuple[float, float] = (1.0, 1.0),
    ) -> GeneratorBuilderReturn:
        shader_params = struct.pack("iiiiffi", shader_shape, out_w, out_h, vertices, float(size), float(line_width), hollow)
        builder = (
            gpu_util.PyImageGenerateBuilder()
            .add_shader(self.figure_shader, shader_params, out_w, out_h)
            .add_shader(self.encode_color_shader, struct.pack("ffff", 1.0, color[0], color[1], color[2]), out_w, out_h)
        )
        item_result = params.item_result.with_size(out_w, out_h).mul_scale(*scale)
        return GeneratorBuilderReturn(builder, item_result)

    def _generate_image(
        self,
        params: VideoGenerateParameters,
        path: str | None,
        size: int,
        width_scale: float,
        height_scale: float,
        color: tuple,
    ) -> GeneratorBuilderReturn | None:
        if not path:
            return None
        self.image_cache.ensure_loaded(path)
        entry = self.image_cache.get(path)
        if entry is None:
            return None
        loader, image_func = entry

        w, h = loader.width, loader.height
        builder = (
            gpu_util.PyImageGenerateBuilder()
            .add_texture_func(image_func, None, w, h)
            .add_shader(self.image_mask_shader, None, w, h)
            .add_shader(self.encode_color_shader, struct.pack("ffff", 1.0, color[0], color[1], color[2]), w, h)
        )
        fit = size / max(w, h)
        item_result = params.item_result.with_size(w, h).mul_scale(fit * width_scale, fit * height_scale)
        return GeneratorBuilderReturn(builder, item_result)
