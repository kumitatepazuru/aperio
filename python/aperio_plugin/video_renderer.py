import os.path
import traceback
from typing import Literal

from aperio import gpu_util, logger
from aperio.item_structures import AdditionalItem, ItemResult, ItemStructure

from .context import AperioContext
from .frame_builder import apply_generate_result, last_leaf_id, layer_params
from .generator_registry import GeneratorRegistry
from .plugin_base.generator_base import (
    GeneratorBuilderReturn,
    GeneratorFuncReturn,
    GeneratorKind,
    GeneratorShaderReturn,
    GeneratorTextureReturn,
    VideoGenerateParameters,
)

type VideoGenerateResult = GeneratorShaderReturn | GeneratorFuncReturn | GeneratorTextureReturn | GeneratorBuilderReturn | ItemResult
type StructureTarget = tuple[str, int, int]
"""`GenerateStructure.id` が指すパイプライン上の出力: (自動採番パイプラインid, width, height)"""

SHADER_DIR = os.path.join(os.path.dirname(__file__), "shaders")


class VideoRenderer:
    """
    フレーム構造からジェネレーターを呼び出してレイヤーごとのパイプラインを組み立て、
    compose シェーダーで1枚のフレームに合成する。
    """

    def __init__(self, context: AperioContext, registry: GeneratorRegistry) -> None:
        self._context = context
        self._registry = registry

        generator = context.image_generator
        with open(os.path.join(SHADER_DIR, "compose.slang"), "r", encoding="utf-8") as f:
            sampler = gpu_util.PySamplerOptions("clamp_to_edge", "linear")
            # レイヤー数はフレームごとに変わるので、入力テクスチャは可変長配列
            self._compose_shader = gpu_util.PyCompiledShader.from_slang(
                "compose_layer",
                f.read(),
                generator,
                search_paths=[SHADER_DIR],
                input_texture_layout="variable",
                sampler_options=sampler,
            )
        with open(os.path.join(SHADER_DIR, "fill_black.slang"), "r", encoding="utf-8") as f:
            self._fill_black_shader = gpu_util.PyCompiledShader.from_slang(
                "fill_black", f.read(), generator, search_paths=[SHADER_DIR]
            )

    def render_buf(
        self,
        frame_number: int,
        frame_structure: list[ItemStructure],
        width: int,
        height: int,
        buffer_ptr: int,
    ) -> dict[str, ItemResult]:
        """フレームを生成し、指定されたバッファに書き込む。各レイヤーの生成結果を返す。"""
        builder, results = self._build(frame_number, frame_structure, width, height)
        self._context.image_generator.generate_buf(builder, buffer_ptr)
        return results

    def render_shared_texture(
        self,
        frame_number: int,
        frame_structure: list[ItemStructure],
        width: int,
        height: int,
        texture_handle: gpu_util.PySharedTextureHandle,
        format: gpu_util.WrappedSharedTextureFormat,
    ) -> dict[str, ItemResult]:
        """フレームを生成し、指定された共有テクスチャに書き込む。各レイヤーの生成結果を返す。"""
        builder, results = self._build(frame_number, frame_structure, width, height)
        self._context.image_generator.generate_shared_texture(builder, texture_handle, format)
        return results

    def _build(
        self,
        frame_number: int,
        frame_structure: list[ItemStructure],
        width: int,
        height: int,
    ) -> tuple[gpu_util.PyImageGenerateBuilder, dict[str, ItemResult]]:
        try:
            if not isinstance(frame_structure, list):
                raise TypeError("frame_structure must be a list of ItemStructure")
            if not isinstance(width, int) or not isinstance(height, int):
                raise TypeError("width and height must be integers")
            if width <= 0 or height <= 0:
                raise ValueError("width and height must be positive integers")

            composition = _FrameComposition(self._registry, frame_number, width, height)
            for layer in frame_structure:
                if not isinstance(layer, ItemStructure.Video):
                    logger.warning(f"Layer {layer.id} is not a video layer. Skipping.") # type: ignore
                    continue
                composition.add_item(layer)

            builder = composition.to_builder(self._compose_shader, self._fill_black_shader)

        except Exception as e:
            logger.error(traceback.format_exc())
            raise RuntimeError(f"Failed to build frame pipeline: {e}") from e

        return builder, composition.results


class _FrameComposition:
    """
    1フレーム分の合成状態。レイヤーごとのビルダーと compose シェーダーへのパラメーター、
    GenerateStructure.id → パイプライン出力の対応表を、フレームの処理中だけ保持する。
    """

    def __init__(self, registry: GeneratorRegistry, frame_number: int, width: int, height: int) -> None:
        self._registry = registry
        self._frame_number = frame_number
        self._width = width
        self._height = height
        # link_id の解決に使う。通常のエントリを処理するたびに、その出力をここに登録する
        self._structure_targets: dict[str, StructureTarget] = {}
        self._layer_builders: list[gpu_util.PyImageGenerateBuilder] = []
        self._layer_params: list[bytes] = []
        self.results: dict[str, ItemResult] = {}

    def add_item(self, layer: ItemStructure.Video) -> None:
        """
        アイテムを1つ解決して合成対象に追加する。チェーン中に現れた追加アイテム(additionalItem)も
        実アイテムと全く同じコードパスで解決し、合成順で背面側/前面側に挿入する。
        """
        chain = self._resolve_chain(layer)
        if chain is None:
            return
        builder, item_result = chain.finish()
        self.results[layer.id] = item_result

        behind_entries = self._resolve_additional(chain.behind, layer.id)
        ahead_entries = self._resolve_additional(chain.ahead, layer.id)

        for entry in behind_entries:
            self._push(*entry)
        self._push(builder, layer, item_result)
        for entry in ahead_entries:
            self._push(*entry)

    def to_builder(
        self, compose_shader: gpu_util.PyCompiledShader, fill_black_shader: gpu_util.PyCompiledShader
    ) -> gpu_util.PyImageGenerateBuilder:
        if not self._layer_builders:
            return gpu_util.PyImageGenerateBuilder().add_shader(fill_black_shader, None, self._width, self._height)

        return (
            gpu_util.PyImageGenerateBuilder()
            .add_parallel(self._layer_builders)
            .add_shader(
                compose_shader, b"".join(self._layer_params), self._width, self._height
            ) # TODO: render Passを使っての高速化と簡潔化を試みる
        )

    def _push(self, builder: gpu_util.PyImageGenerateBuilder, item: "ItemStructure.Video", result: ItemResult) -> None:
        self._layer_builders.append(builder)
        self._layer_params.append(layer_params(item, result, self._height))

    def _resolve_additional(
        self, items: list[AdditionalItem], layer_id: str
    ) -> list[tuple[gpu_util.PyImageGenerateBuilder, "ItemStructure.Video", ItemResult]]:
        entries = []
        for additional in items:
            if not isinstance(additional.item, ItemStructure.Video):
                logger.warning(f"additional_item of layer {layer_id} is not a video item. Skipping.")
                continue
            chain = self._resolve_chain(additional.item)
            if chain is None:
                continue
            builder, item_result = chain.finish()
            entries.append((builder, additional.item, item_result))
        return entries

    def _resolve_chain(self, layer: ItemStructure.Video) -> _VideoChain | None:
        """
        1つの Video アイテムの object→effects チェーンを解決する。
        オブジェクトのジェネレーターが None を返した場合(スキップ対象)は None を返す。
        """
        chain = _VideoChain(self._structure_targets)

        obj = layer.object
        if obj.get("link_id") is not None:
            chain.link(obj["id"], obj["link_id"])
        else:
            result = self._generate(GeneratorKind.VIDEO_OBJECT, layer, obj, self._width, self._height)
            if result is None:
                return None
            chain.apply(obj["id"], result)

        for effect in layer.effects:
            if effect.get("link_id") is not None:
                chain.link(effect["id"], effect["link_id"])
                continue
            current = chain.item_result
            result = self._generate(GeneratorKind.VIDEO_EFFECT, layer, effect, current.width, current.height)
            if result is not None:
                chain.apply(effect["id"], result)

        return chain

    def _generate(
        self,
        kind: Literal[GeneratorKind.VIDEO_OBJECT, GeneratorKind.VIDEO_EFFECT],
        layer: ItemStructure.Video,
        structure: dict,
        width: int,
        height: int,
    ) -> VideoGenerateResult | None:
        """
        structure(GenerateStructure) に対応するジェネレーターを呼び出す。

        Args:
            width, height: 生成元のサイズ。オブジェクトの場合はフレームサイズ、エフェクトの場合は直前までのオブジェクトサイズ
        """
        generator = self._registry.get(structure["name"], kind)
        params = VideoGenerateParameters(
            frame_number=self._frame_number,
            layer=layer,
            args=structure["parameters"],
            width=width,
            height=height,
            structure_id=structure["id"],
        )
        return generator.generate(params)


class _VideoChain:
    """
    1アイテム分の object→effects チェーンの途中状態。ビルダー・合成後の ItemResult・チェーン中に現れた追加アイテムを積み上げていく。
    """

    def __init__(self, structure_targets: dict[str, StructureTarget]) -> None:
        self._structure_targets = structure_targets
        self._builder = gpu_util.PyImageGenerateBuilder()
        self._item_result: ItemResult | None = None
        # link_idが連続するチェーンでは、実際にadd_linkedをビルダーに反映するのは
        # その連続run内の最後のstepのときだけにする(中間のstepは_structure_targetsへの
        # エイリアス登録のみ)。Linkedは直前の状態を無視してリンク先のテクスチャに
        # 差し替えるため、連続してadd_linkedすると手前の分が無駄になってしまうため。
        self._pending_link: StructureTarget | None = None
        self.behind: list[AdditionalItem] = []
        self.ahead: list[AdditionalItem] = []

    @property
    def item_result(self) -> ItemResult:
        if self._item_result is None:
            raise RuntimeError("The chain has no object step yet")
        return self._item_result

    def link(self, structure_id: str, link_id: str) -> None:
        """プラグインを呼ばず、link_idが指す既存のパイプライン出力をそのまま使い回すステップ。"""
        target = self._structure_targets.get(link_id)
        if target is None:
            raise ValueError(f"link_id {link_id} was not resolved before use (structure {structure_id})")
        self._structure_targets[structure_id] = target
        self._pending_link = target
        self._item_result = ItemResult(target[1], target[2])

    def apply(self, structure_id: str, result: VideoGenerateResult) -> None:
        """ジェネレーターの生成結果をチェーンに積み、その出力を `structure_id` で参照できるよう登録する。"""
        step_result = result if isinstance(result, ItemResult) else result.item_result
        additional = step_result.additional_item
        if additional is not None:
            (self.behind if additional.behind else self.ahead).append(additional)

        if self._item_result is None:
            self._item_result = step_result
        else:
            self._item_result.combine(step_result)

        self._flush_link()
        if not isinstance(result, ItemResult):
            self._builder = apply_generate_result(self._builder, result)

        id_tree = self._builder.get_id_tree()
        if id_tree:  # get_id_treeは常にlist、空ならNoneではなく空listが返る
            self._structure_targets[structure_id] = (last_leaf_id(id_tree), self._item_result.width, self._item_result.height)

    def finish(self) -> tuple[gpu_util.PyImageGenerateBuilder, ItemResult]:
        """チェーン末尾がlinkで終わる場合はここで materialize して、ビルダーと最終 ItemResult を返す。"""
        self._flush_link()
        return self._builder, self.item_result

    def _flush_link(self) -> None:
        if self._pending_link is not None:
            self._builder = self._builder.add_linked(*self._pending_link)
            self._pending_link = None
