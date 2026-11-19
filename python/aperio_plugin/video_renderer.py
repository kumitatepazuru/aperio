import os.path
import traceback
from typing import Literal

from aperio import gpu_util, logger
from aperio.item_structures import AdditionalItem, GeneratorEvent, ItemResult, ItemStructure

from .blend_mode import BLEND_SHADER_DIR, DEFAULT_BLEND_MODE_ID, BlendModeRegistry
from .context import AperioContext
from .event_manager import EventDispatcher
from .frame_builder import apply_generate_result, blend_layer_params, last_leaf_id
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


def _read_shader(path: str) -> str:
    with open(path, "r", encoding="utf-8") as f:
        return f.read()


class VideoRenderer:
    """
    フレーム構造からジェネレーターを呼び出してレイヤーごとのパイプラインを組み立て、
    各レイヤーのブレンドモードで黒背景のフレームへ順に合成する。
    """

    def __init__(
        self,
        context: AperioContext,
        registry: GeneratorRegistry,
        blend_modes: BlendModeRegistry,
        events: EventDispatcher,
    ) -> None:
        self._context = context
        self._registry = registry
        self._events = events

        generator = context.image_generator
        self._fill_black_shader = gpu_util.PyCompiledShader.from_slang(
            "fill_black", _read_shader(os.path.join(SHADER_DIR, "fill_black.slang")), generator, search_paths=[SHADER_DIR]
        )
        self._blender = _LayerBlender(
            blend_modes,
            fallback_shader=gpu_util.PyCompiledShader.from_slang_raster(
                "aperio_blend_fallback_normal",
                _read_shader(os.path.join(BLEND_SHADER_DIR, "fallback_normal.slang")),
                generator,
                search_paths=[BLEND_SHADER_DIR],
                sampler_options=gpu_util.PySamplerOptions("clamp_to_edge", "linear"),
                vertex_entry="vs_main",
                fragment_entry="fs_main",
                vertex_count=6,
            ),
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

            frame_structure = self._events.reduce_event(GeneratorEvent.BeforeMakeFrame, frame_structure)

            composition = _FrameComposition(self._registry, frame_number, width, height)
            for layer in frame_structure:
                if not isinstance(layer, ItemStructure.Video):
                    logger.warning(f"Layer {layer.id} is not a video layer. Skipping.") # type: ignore
                    continue
                composition.add_item(layer)

            builder = composition.to_builder(self._fill_black_shader, self._blender)
            results = self._events.reduce_event(GeneratorEvent.AfterMakeFrame, composition.results, frame_structure)

        except Exception as e:
            logger.error(traceback.format_exc())
            raise RuntimeError(f"Failed to build frame pipeline: {e}") from e

        return builder, results


class _LayerBlender:
    """
    ItemResult.blend_mode の id からブレンドモードを選び、レイヤーを編集中のフレームへ合成するステップを組み立てる。
    """

    def __init__(self, blend_modes: BlendModeRegistry, fallback_shader: gpu_util.PyCompiledShader) -> None:
        self._blend_modes = blend_modes
        # アイテムのブレンドモードの id が登録されていないときに使う組み込みの通常合成
        self._fallback_shader = fallback_shader
        # 毎フレーム同じ警告を出さないよう、警告済みの未登録 id を覚えておく
        self._warned_ids: set[str] = set()

    def blend(
        self,
        frame: gpu_util.PyImageGenerateBuilder,
        layer: gpu_util.PyImageGenerateBuilder,
        result: ItemResult,
        width: int,
        height: int,
    ) -> gpu_util.PyImageGenerateBuilder:
        """
        frame(編集中のフレームを出力するビルダー)に layer を合成したフレームを出力するビルダーを返す。
        """
        params = blend_layer_params(result, height)
        shader = self._resolve(result.blend_mode)
        # 入力 = [編集中のフレーム, レイヤー]。出力はフレームのコピーを下地にした render pass
        return gpu_util.PyImageGenerateBuilder().add_parallel([frame, layer]).add_shader(shader, params, width, height)

    def _resolve(self, blend_mode: str) -> gpu_util.PyCompiledShader:
        mode = self._blend_modes.get(blend_mode)
        if mode is not None:
            return mode.shader
        # 既定の id はプラグインが登録するまで組み込みで描くのが正常なので、警告しない
        if blend_mode != DEFAULT_BLEND_MODE_ID and blend_mode not in self._warned_ids:
            self._warned_ids.add(blend_mode)
            logger.warning(f"Blend mode '{blend_mode}' is not registered. Falling back to the normal blend.")
        return self._fallback_shader


class _FrameComposition:
    """
    1フレーム分の合成状態。合成順に並べたレイヤーのビルダーと ItemResult、
    GenerateStructure.id → パイプライン出力の対応表を、フレームの処理中だけ保持する。
    """

    def __init__(self, registry: GeneratorRegistry, frame_number: int, width: int, height: int) -> None:
        self._registry = registry
        self._frame_number = frame_number
        self._width = width
        self._height = height
        # link_id の解決に使う。通常のエントリを処理するたびに、その出力をここに登録する
        self._structure_targets: dict[str, StructureTarget] = {}
        self._layers: list[tuple[gpu_util.PyImageGenerateBuilder, ItemResult]] = []
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

        additional_items = item_result.additional_items
        behind_entries = self._resolve_additional([a for a in additional_items if a.behind], layer.id)
        ahead_entries = self._resolve_additional([a for a in additional_items if not a.behind], layer.id)

        for entry in behind_entries:
            self._push(*entry)
        self._push(builder, item_result)
        for entry in ahead_entries:
            self._push(*entry)

    def to_builder(
        self, fill_black_shader: gpu_util.PyCompiledShader, blender: _LayerBlender
    ) -> gpu_util.PyImageGenerateBuilder:
        """黒で塗ったフレームから始めて、各レイヤーを合成順(背面→前面)にブレンドモードで重ねるビルダーを返す。"""
        frame = gpu_util.PyImageGenerateBuilder().add_shader(fill_black_shader, None, self._width, self._height)
        for builder, result in self._layers:
            frame = blender.blend(frame, builder, result, self._width, self._height)
        return frame

    def _push(self, builder: gpu_util.PyImageGenerateBuilder, result: ItemResult) -> None:
        # 描画するステップが無い(ItemResult だけを返すグループ等)ものは、合成するテクスチャが無い
        if not builder.get_id_tree():
            return
        self._layers.append((builder, result))

    def _resolve_additional(
        self, items: list[AdditionalItem], layer_id: str
    ) -> list[tuple[gpu_util.PyImageGenerateBuilder, ItemResult]]:
        entries = []
        for additional in items:
            if not isinstance(additional.item, ItemStructure.Video):
                logger.warning(f"additional_item of layer {layer_id} is not a video item. Skipping.")
                continue
            chain = self._resolve_chain(additional.item)
            if chain is None:
                continue
            entries.append(chain.finish())
        return entries

    def _resolve_chain(self, layer: ItemStructure.Video) -> _VideoChain | None:
        """
        1つの Video アイテムの object→effects チェーンを解決する。
        オブジェクトのジェネレーターが None を返した場合(スキップ対象)は None を返す。
        """
        chain = _VideoChain(self._structure_targets, layer)

        obj = layer.object
        if obj.get("link_id") is not None:
            chain.link(obj["id"], obj["link_id"])
        else:
            result = self._generate(GeneratorKind.VIDEO_OBJECT, layer, obj, self._width, self._height, None)
            if result is None:
                return None
            chain.apply(obj["id"], result)

        if not chain.has_pipeline:
            # テクスチャを持たないオブジェクト(グループ等)には、エフェクトを適用する対象が無い
            return chain

        for effect in layer.effects:
            if effect.get("link_id") is not None:
                chain.link(effect["id"], effect["link_id"])
                continue
            # プラグインが変更してから None を返しても(スキップ)累積値が汚れないようにコピーを渡す
            current = chain.item_result.copy()
            result = self._generate(GeneratorKind.VIDEO_EFFECT, layer, effect, current.width, current.height, current)
            if result is not None:
                chain.apply(effect["id"], result)

        return chain

    def _generate(
        self,
        kind: Literal[GeneratorKind.VIDEO_OBJECT, GeneratorKind.VIDEO_EFFECT],
        item: ItemStructure.Video,
        structure: dict,
        width: int,
        height: int,
        item_result: ItemResult | None,
    ) -> VideoGenerateResult | None:
        """
        structure(GenerateStructure) に対応するジェネレーターを呼び出す。

        Args:
            width, height: 生成元のサイズ。オブジェクトの場合はフレームサイズ、エフェクトの場合は直前までのオブジェクトサイズ
            item_result: 直前までの累積結果。None(オブジェクトの場合)ならアイテム自身から作る
        """
        if item_result is None:
            item_result = ItemResult.from_item(item)
        generator = self._registry.get(structure["name"], kind)
        params = VideoGenerateParameters(
            frame_number=self._frame_number,
            item=item,
            args=structure["parameters"],
            width=width,
            height=height,
            structure_id=structure["id"],
            item_result=item_result,
            canvas_width=self._width,
            canvas_height=self._height,
        )
        return generator.generate(params)


class _VideoChain:
    """
    1アイテム分の object→effects チェーンの途中状態。ビルダーと、各ステップが返した最新の ItemResultを持つ。
    """

    def __init__(self, structure_targets: dict[str, StructureTarget], layer: ItemStructure.Video) -> None:
        self._structure_targets = structure_targets
        self._layer = layer
        self._builder = gpu_util.PyImageGenerateBuilder()
        self._item_result: ItemResult | None = None
        # link_idが連続するチェーンでは、実際にadd_linkedをビルダーに反映するのは
        # その連続run内の最後のstepのときだけにする(中間のstepは_structure_targetsへの
        # エイリアス登録のみ)。Linkedは直前の状態を無視してリンク先のテクスチャに
        # 差し替えるため、連続してadd_linkedすると手前の分が無駄になってしまうため。
        self._pending_link: StructureTarget | None = None

    @property
    def item_result(self) -> ItemResult:
        if self._item_result is None:
            raise RuntimeError("The chain has no object step yet")
        return self._item_result

    @property
    def has_pipeline(self) -> bool:
        """描画するステップ(またはリンク)を持っているか。ItemResult だけを返したオブジェクトは持たない。"""
        return self._pending_link is not None or bool(self._builder.get_id_tree())

    def link(self, structure_id: str, link_id: str) -> None:
        """プラグインを呼ばず、link_idが指す既存のパイプライン出力をそのまま使い回すステップ。"""
        target = self._structure_targets.get(link_id)
        if target is None:
            raise ValueError(f"link_id {link_id} was not resolved before use (structure {structure_id})")
        self._structure_targets[structure_id] = target
        self._pending_link = target
        # リンク先のテクスチャに差し替わるので、変形はアイテム自身のものからやり直す。additional_itemsは引き継ぐ
        linked = ItemResult.from_item(self._layer, target[1], target[2])
        if self._item_result is not None:
            linked.additional_items = self._item_result.additional_items
        self._item_result = linked

    def apply(self, structure_id: str, result: VideoGenerateResult) -> None:
        """ジェネレーターの生成結果(絶対値)で累積値を置き換え、その出力を `structure_id` で参照できるよう登録する。"""
        self._item_result = result if isinstance(result, ItemResult) else result.item_result

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
