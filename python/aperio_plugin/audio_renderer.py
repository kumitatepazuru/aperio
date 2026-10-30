import math
import traceback

import numpy as np
import numpy.typing as npt

from aperio import logger
from aperio.item_structures import AdditionalItem, ItemStructure

from .audio_mixer import AudioMixBuffer
from .context import AperioContext
from .generator_registry import GeneratorRegistry
from .plugin_base.generator_base import AudioGenerateParameters, AudioGeneratorReturn, GeneratorKind


class AudioRenderer:
    """
    オーディオ構造からジェネレーターを呼び出してサンプルを生成・ミックスし、再生キューに積む。
    """

    def __init__(self, context: AperioContext, registry: GeneratorRegistry) -> None:
        self._context = context
        self._registry = registry

    def play(self, audio_structure: list[ItemStructure], sample_rate: int, channels: int, start_time: float, duration: float) -> None:
        """
        指定されたオーディオ構造に基づいてオーディオを生成し、再生する。

        Args:
            audio_structure (list[ItemStructure]): オーディオ構造のリスト
            sample_rate (int): サンプルレート
            channels (int): チャンネル数
            start_time (float): 再生を開始する時間（秒）
            duration (float): 再生する期間（秒）
        """
        samples = self.render(audio_structure, sample_rate, channels, start_time, math.floor(sample_rate * duration))
        start_sample = round(start_time * sample_rate)
        self._context.audio_manager.stack_audio(samples, start_sample)

    def render(self, audio_structure: list[ItemStructure], sample_rate: int, channels: int, start_time: float, sample_count: int) -> npt.NDArray[np.float32]:
        """
        指定されたオーディオ構造に基づいてオーディオサンプルを生成する。

        Args:
            audio_structure (list[ItemStructure]): オーディオ構造のリスト
            sample_rate (int): サンプルレート
            channels (int): チャンネル数
            start_time (float): 生成を開始する時間（秒）
            sample_count (int): 生成するサンプル数

        Returns:
            npt.NDArray[np.float32]: 生成されたオーディオサンプルの2次元配列（チャンネル数 x サンプル数）
        """
        try:
            fps = self._context.store_manager.get_state().frame_state.fps

            buffer = AudioMixBuffer(channels, sample_count)
            request_end_time = start_time + sample_count / sample_rate

            for layer in audio_structure:
                if not isinstance(layer, ItemStructure.Audio):
                    logger.warning(f"Layer {layer.id} is not an audio layer. Skipping.") # type: ignore
                    continue

                # フレームを時間に変換し、リクエスト時間範囲とのオーバーラップを計算する
                overlap_start = max(start_time, layer.start / fps)
                overlap_end = min(request_end_time, layer.end / fps)

                if overlap_start >= overlap_end:
                    logger.warning(f"Layer {layer.id} has no overlap with requested audio range. Skipping.")
                    continue

                # 出力バッファ内のサンプルオフセットと生成サンプル数
                sample_offset = round((overlap_start - start_time) * sample_rate)
                overlap_sample_count = round((overlap_end - overlap_start) * sample_rate)

                if overlap_sample_count <= 0:
                    logger.warning(f"Layer {layer.id} has no overlapping samples to generate. Skipping.")
                    continue

                processed = self._process_item(layer, overlap_start, sample_rate, channels, overlap_sample_count)
                if processed is None:
                    continue
                layer_result, additional_items = processed
                buffer.add(layer.object["name"], layer_result.samples, overlap_sample_count, layer.volume, layer.pan, sample_offset)

                # additionalItem: 同じ時間窓に加算ミックスする追加のオーディオアイテムがあれば、
                # それぞれ実アイテムと全く同じ _process_item で解決してから同じ窓に加算する
                # (音声は加算合成なので順序に意味が無く、behind は無視してよい。
                for additional in additional_items:
                    if not isinstance(additional.item, ItemStructure.Audio):
                        logger.warning(f"additional_item of layer {layer.id} is not an audio item. Skipping.")
                        continue
                    additional_processed = self._process_item(
                        additional.item, overlap_start, sample_rate, channels, overlap_sample_count
                    )
                    if additional_processed is not None:
                        additional_result, _ = additional_processed
                        buffer.add(
                            additional.item.object["name"],
                            additional_result.samples,
                            overlap_sample_count,
                            additional.item.volume,
                            additional.item.pan,
                            sample_offset,
                        )

            return buffer.finish()

        except Exception as e:
            logger.error(traceback.format_exc())
            raise RuntimeError(f"Failed to generate audio sample: {e}") from e

    def _process_item(
        self,
        layer: "ItemStructure.Audio",
        start_time: float,
        sample_rate: int,
        channels: int,
        sample_count: int,
    ) -> tuple[AudioGeneratorReturn, list[AdditionalItem]] | None:
        """
        1つの Audio アイテムの object→effects チェーンを解決し、最終 AudioGeneratorReturn と
        チェーン中に現れた追加アイテムの配列を返す。

        render のメインループ(実アイテム)と、additionalItem 経由で
        流し込まれる合成アイテムの両方から共通で呼ばれる。
        """
        additional_items: list[AdditionalItem] = []

        obj_plugin = self._registry.get(layer.object["name"], GeneratorKind.AUDIO_OBJECT)
        obj_params = AudioGenerateParameters(
            start_time=start_time,
            layer=layer,
            sample_rate=sample_rate,
            channels=channels,
            sample_count=sample_count,
            args=layer.object["parameters"],
        )
        layer_result = obj_plugin.generate(obj_params)
        if layer_result is None:
            return None
        if layer_result.additional_item is not None:
            additional_items.append(layer_result.additional_item)

        # エフェクトを順番に適用（各エフェクトは前段サンプルをinput_samplesで受け取る）
        for effect in layer.effects:
            effect_plugin = self._registry.get(effect["name"], GeneratorKind.AUDIO_EFFECT)
            effect_params = AudioGenerateParameters(
                start_time=start_time,
                layer=layer,
                sample_rate=sample_rate,
                channels=channels,
                sample_count=sample_count,
                args=effect["parameters"],
                input_samples=layer_result.samples,
            )
            tmp_result = effect_plugin.generate(effect_params)
            if tmp_result is None:
                return None
            layer_result = tmp_result
            if layer_result.additional_item is not None:
                additional_items.append(layer_result.additional_item)

        return layer_result, additional_items
