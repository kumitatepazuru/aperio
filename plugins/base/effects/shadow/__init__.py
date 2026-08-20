import struct
import uuid
from dataclasses import dataclass

import aperio_plugin
from aperio import gpu_util
from aperio.gpu_util import PyCompiledWgsl
from aperio.item_structures import (
    AdditionalItem,
    FileFilter,
    GenerateStructure,
    GeneratorEvent,
    GeneratorInformation,
    ItemResult,
    ItemStructure,
    RequestStructureParameter,
)
from aperio_plugin.event_manager import event
from aperio_plugin.plugin_base.generator_base import GeneratorBuilderReturn, VideoEffectGeneratorBase, VideoGenerateParameters
from aperio_plugin.utils import build_link_prefix

from ...common.params import make_generator_information, pack_box_average_dir_params, pack_expand_params
from ...common.pattern_image import PATTERN_EXTENSIONS, PatternImageCache
from ...common.shader_loader import compose_common_shader, effect_dirs, lib_module, shared_shader

# 「影を別オブジェクトで描画」時、影レイヤーの計算だけを行う非表示エフェクトの名前。
# ShadowEffect自身をチェーン末尾に再度置くと`separate_object`分岐に再突入して無限再帰する
# ため、`separate_object`という概念を一切知らない別プラグインとして切り出している。
_SHADOW_LAYER_EFFECT_NAME = "base_effect._shadow_layer_effect"


def _clamp_axis(offset: int, size: int, r: int, max_dim: int) -> tuple[int, int]:
    """README §2 のオフセット・半径クランプ。オフセットが先に半径の予算を食う。"""
    budget = max(0, max_dim - size)
    if abs(offset) > budget:
        offset = (1 if offset >= 0 else -1) * budget
    remaining = budget - abs(offset)
    return offset, max(0, min(r, remaining // 2))


@dataclass
class _ShadowLayerShaders:
    """影レイヤー(silhouette→ぼかし→着色/パターン)の計算に必要なシェーダー一式。
    `ShadowEffect`と`_ShadowLayerEffect`の双方が`__init__`で自前にコンパイルして持つ
    (シェーダーコンパイルは起動時に1回だけのコストなので、複製しても問題ない)。"""

    expand: PyCompiledWgsl
    box_average_dir: PyCompiledWgsl
    encode_color: PyCompiledWgsl
    encode_pattern: PyCompiledWgsl
    tile: PyCompiledWgsl


def _compile_shadow_layer_shaders(source_file: str) -> _ShadowLayerShaders:
    _, common_dir = effect_dirs(source_file)
    blur_module = lib_module(common_dir, "blur")
    return _ShadowLayerShaders(
        expand=shared_shader("expand", common_dir, "expand.wgsl"),
        box_average_dir=compose_common_shader(
            "shadow_box_average_dir", [blur_module], common_dir, "box_average_dir.wgsl"
        ),
        encode_color=shared_shader("encode_color", common_dir, "encode_color.wgsl"),
        encode_pattern=shared_shader("encode_pattern", common_dir, "encode_pattern.wgsl"),
        tile=shared_shader("tile", common_dir, "tile.wgsl"),
    )


@dataclass
class _ShadowParams:
    density: float
    x: int
    y: int
    r: int
    color: tuple
    pattern_path: str


def _derive_shadow_params(args: dict, w: int, h: int, max_dim: int) -> _ShadowParams | None:
    """UIパラメータから、クランプ済みのX/Y/半径を含む影パラメータを導出する。
    濃さ=0のときはNoneを返す(README §2、画像にもキャンバスにも一切触れない)。"""
    strength_ui = args.get("strength", 40.0)
    if strength_ui <= 0:
        return None
    density = strength_ui / 100.0

    # X/Y/半径は_clamp_axis()がmax_dim由来の実バッファ予算で再クランプするため、
    # ここでは整数化のみ行う。
    X = round(args.get("x", -40))
    Y = round(args.get("y", 24))
    r = max(0, round(args.get("diffusion", 10)))
    color = args.get("color", (0.0, 0.0, 0.0, 1.0))
    pattern_paths = args.get("pattern_path", [])
    pattern_path = pattern_paths[0] if pattern_paths else ""

    # クランプ(README §2)。Xが先に半径の予算を食い、Yがそれを引き継ぐ。
    X, r = _clamp_axis(X, w, r, max_dim)
    Y, r = _clamp_axis(Y, h, r, max_dim)

    return _ShadowParams(density=density, x=X, y=Y, r=r, color=color, pattern_path=pattern_path)


@dataclass
class ShadowLayerResult:
    builder: gpu_util.PyImageGenerateBuilder
    box_w: int
    box_h: int


def _build_shadow_layer(
    shaders: _ShadowLayerShaders,
    pattern_cache: PatternImageCache,
    shadow_params: _ShadowParams,
    w: int,
    h: int,
) -> ShadowLayerResult:
    """クランプ済みの影パラメータから、影レイヤー(box_w x box_h、ストレートアルファ)の
    builderを組み立てる。影の起点となるシルエットは、暗黙のパイプライン合流
    (add_parallel_wgsl経由で前段のstateを継承する仕組み)によって、呼び出し元の直前までの
    累積結果から引き継がれる。"""
    density, r, color, pattern_path = (
        shadow_params.density,
        shadow_params.r,
        shadow_params.color,
        shadow_params.pattern_path,
    )

    # 半径の2分割(README §2)。r1<=r2、r1が最後(最終エンコード直前)のパスに使われる。
    r1 = r // 2
    r2 = r - r1
    box_w, box_h = w + 2 * r, h + 2 * r

    entry = pattern_cache.get(pattern_path) if pattern_path else None
    loader, pattern_func = entry if entry is not None else (None, None)

    mask_chain = gpu_util.PyImageGenerateBuilder().add_wgsl(
        shaders.expand, pack_expand_params(r, r, box_w, box_h), box_w, box_h
    )
    for radius, step_x, step_y in ((r2, 0, 1), (r2, 1, 0), (r1, 0, 1), (r1, 1, 0)):
        mask_chain = mask_chain.add_wgsl(
            shaders.box_average_dir,
            pack_box_average_dir_params(radius, step_x, step_y, box_w, box_h),
            box_w,
            box_h,
        )

    if loader is not None and pattern_func is not None:
        tiled_branch = (
            gpu_util.PyImageGenerateBuilder()
            .add_texture_func(pattern_func, None, loader.width, loader.height)
            .add_wgsl(shaders.tile, struct.pack("ii", box_w, box_h), box_w, box_h)
        )
        shadow_layer = (
            gpu_util.PyImageGenerateBuilder()
            .add_parallel_wgsl([mask_chain, tiled_branch])
            .add_wgsl(shaders.encode_pattern, struct.pack("f", density), box_w, box_h)
        )
    else:
        shadow_layer = mask_chain.add_wgsl(
            shaders.encode_color,
            struct.pack("ffff", density, color[0], color[1], color[2]),
            box_w,
            box_h,
        )

    return ShadowLayerResult(builder=shadow_layer, box_w=box_w, box_h=box_h)


class ShadowEffect(VideoEffectGeneratorBase):
    def __init__(self) -> None:
        super().__init__()
        self.name = "base_effect.shadow_effect"
        self.display_name = "シャドー"
        self.description = "Draws a blurred, colored (or pattern-filled) copy of the object's alpha silhouette offset behind it."

        _, common_dir = effect_dirs(__file__)
        self.shaders = _compile_shadow_layer_shaders(__file__)
        self.select_shader = shared_shader("shadow_select", common_dir, "select.wgsl")
        self.composite_shader = shared_shader("composite", common_dir, "composite.wgsl")

        self.pattern_cache = PatternImageCache("shadow_pattern")

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
                    id="x",
                    title="X",
                    default_value=-40,
                    suffix="px",
                    min=-200,
                    max=200,
                ),
                RequestStructureParameter.Int(
                    id="y",
                    title="Y",
                    default_value=24,
                    suffix="px",
                    min=-200,
                    max=200,
                ),
                RequestStructureParameter.Float(
                    id="strength",
                    title="濃さ",
                    default_value=40.0,
                    suffix="%",
                    min=0.0,
                    max=100.0,
                ),
                RequestStructureParameter.Int(
                    id="diffusion",
                    title="拡散",
                    default_value=10,
                    suffix="px",
                    min=0,
                    max=50,
                ),
                RequestStructureParameter.Color(
                    id="color",
                    title="影色の設定",
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
                RequestStructureParameter.Bool(
                    id="separate_object",
                    title="影を別オブジェクトで描画",
                    default_value=False,
                ),
            ],
        )

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn | None:
        args = params.args
        w, h = params.width, params.height
        max_dim = aperio_plugin.image_generator.maximum_texture_size

        # 濃さ=0は実機と同じく即return(README §2、画像にもキャンバスにも一切触れない)。
        shadow_params = _derive_shadow_params(args, w, h, max_dim)
        if shadow_params is None:
            return None

        if bool(args.get("separate_object", False)):
            return self._generate_separate_object(params, w, h, shadow_params)

        shadow = _build_shadow_layer(self.shaders, self.pattern_cache, shadow_params, w, h)

        # --- 通常合成(README §3) ---
        X, Y, r = shadow_params.x, shadow_params.y, shadow_params.r
        nw, nh = w + abs(X) + 2 * r, h + abs(Y) + 2 * r
        shadow_box_x, shadow_box_y = max(X, 0), max(Y, 0)
        obj_x, obj_y = max(-X, 0) + r, max(-Y, 0) + r

        shadow_full_branch = shadow.builder.add_wgsl(
            self.shaders.expand, pack_expand_params(shadow_box_x, shadow_box_y, nw, nh), nw, nh
        )
        object_full_branch = gpu_util.PyImageGenerateBuilder().add_wgsl(
            self.shaders.expand, pack_expand_params(obj_x, obj_y, nw, nh), nw, nh
        )
        final_builder = (
            gpu_util.PyImageGenerateBuilder()
            .add_parallel_wgsl([object_full_branch, shadow_full_branch])
            .add_wgsl(self.composite_shader, None, nw, nh)
        )
        # キャンバスは|X|/|Y|ぶん片側にだけ伸びる(2rは対称)ので、オブジェクト自身の
        # 見た目の位置・回転/拡縮の基点(compose.wgslのcenter_x/center_y)が
        # ずれないよう半分だけ戻す(README §3「fpip+0xD4/+0xD8の補正」に相当)。
        center_x = round(-X / 2)
        center_y = round(-Y / 2)
        return GeneratorBuilderReturn(final_builder, ItemResult(nw, nh, center_x=center_x, center_y=center_y))

    def _generate_separate_object(
        self, params: VideoGenerateParameters, w: int, h: int, shadow_params: _ShadowParams
    ) -> GeneratorBuilderReturn:
        """影を別オブジェクトで描画(README §7)。

        元のオブジェクトは1バイトも変わらずに素通しし、影は独立した`AdditionalItem`として
        背面に挿入する。影のシルエット元データは、オーナーの`object`+このシャドー自身より
        前の`effects`を`link_id`経由の軽量スタブとして切り出し(`build_link_prefix`)、
        その末尾に非表示の`_shadow_layer_effect`を足した構造として組み立てる。
        フレーム内での使い回し(`PluginManager._process_video_item`のlink_id処理)により、
        オーナーの再デコード・再生成は発生しない。
        """
        owner = params.layer
        object_stub, effect_stubs = build_link_prefix(owner, params.structure_id)
        tail_effect = GenerateStructure(
            id=str(uuid.uuid4()),
            name=_SHADOW_LAYER_EFFECT_NAME,
            display_name="",
            parameters=dict(params.args),
        )

        shadow_item = ItemStructure.Video(
            id=str(uuid.uuid4()),
            layer=owner.layer,
            start=owner.start,
            end=owner.end,
            min=None,
            max=None,
            # 影レイヤーの内容自体にはオフセットが焼き込まれていないため、アイテムの位置で
            # ずらす(README §7)。
            x=owner.x + shadow_params.x,
            y=owner.y + shadow_params.y,
            scale=owner.scale,
            rotation=owner.rotation,
            alpha=owner.alpha,
            object=object_stub,
            effects=[*effect_stubs, tail_effect],
        )
        additional_item = AdditionalItem(item=shadow_item, behind=True)

        identity_builder = gpu_util.PyImageGenerateBuilder().add_wgsl(self.select_shader, struct.pack("i", 0), w, h)
        return GeneratorBuilderReturn(identity_builder, ItemResult(w, h, additional_item=additional_item))


class ShadowLayerEffect(VideoEffectGeneratorBase):
    """`ShadowEffect`の「影を別オブジェクトで描画」が使う非表示エフェクト。影レイヤー
    (silhouette→ぼかし→着色/パターン)の計算だけを行い、`separate_object`という概念は
    一切持たない(`ShadowEffect`自身をチェーン末尾に置くと再帰してしまうため分離している)。
    UI のエフェクト追加メニューには出さない(is_hidden=True)。"""

    def __init__(self) -> None:
        super().__init__()
        self.name = _SHADOW_LAYER_EFFECT_NAME
        self.display_name = "(内部)影レイヤー"
        self.description = "influence effectの「影を別オブジェクトで描画」が内部で使う非表示のエフェクトプラグイン。"
        self.is_hidden = True

        self.shaders = _compile_shadow_layer_shaders(__file__)
        self.pattern_cache = PatternImageCache("shadow_pattern_layer")

    @event(type=GeneratorEvent.New)
    @event(type=GeneratorEvent.RequestStructure)
    def on_request_structure(self, params: dict) -> GeneratorInformation:
        paths = params.get("pattern_path", [])
        if paths:
            self.pattern_cache.ensure_loaded(paths[0])
        return GeneratorInformation(display_name=self.display_name, duration_frames=None, max_frame=None, min_frame=None, structure=[])

    def generate(self, params: VideoGenerateParameters) -> GeneratorBuilderReturn | None:
        args = params.args
        w, h = params.width, params.height
        max_dim = aperio_plugin.image_generator.maximum_texture_size

        shadow_params = _derive_shadow_params(args, w, h, max_dim)
        if shadow_params is None:
            return None
        shadow = _build_shadow_layer(self.shaders, self.pattern_cache, shadow_params, w, h)
        return GeneratorBuilderReturn(shadow.builder, ItemResult(shadow.box_w, shadow.box_h))
