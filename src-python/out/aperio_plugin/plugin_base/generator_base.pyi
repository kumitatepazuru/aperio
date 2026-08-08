import numpy as np
import numpy.typing as npt
from . import SubPluginBase as SubPluginBase
from aperio.gpu_util import PyCompiledFunc as PyCompiledFunc, PyCompiledTextureFunc as PyCompiledTextureFunc, PyCompiledWgsl as PyCompiledWgsl, PyImageGenerateBuilder as PyImageGenerateBuilder
from aperio.item_structures import AdditionalItem as AdditionalItem, ItemResult as ItemResult, ItemStructure as ItemStructure
from dataclasses import dataclass

@dataclass
class GeneratorWgslReturn:
    compiled: PyCompiledWgsl
    params: bytes
    item_result: ItemResult

@dataclass
class GeneratorFuncReturn:
    compiled: PyCompiledFunc
    params: object
    item_result: ItemResult

@dataclass
class GeneratorTextureReturn:
    compiled: PyCompiledTextureFunc
    params: object
    item_result: ItemResult

@dataclass
class GeneratorBuilderReturn:
    builder: PyImageGenerateBuilder
    item_result: ItemResult

@dataclass
class VideoGenerateParameters:
    """ビデオフレーム生成に必要なパラメーターをまとめたデータクラス。ジェネレーターのgenerateメソッドに渡される。"""
    frame_number: int
    layer: ItemStructure.Video
    args: dict
    width: int
    height: int
    structure_id: str = ...

@dataclass
class AudioGeneratorReturn:
    """オーディオの generate() の戻り値。additional_item は video 側の
    ItemResult.additional_item と対称な仕組みで、自分と同じ時間窓に
    加算ミックスする追加のオーディオアイテムを指定できる(behind は無視される。
    音声は加算合成なので順序に意味が無いため)。"""
    samples: npt.NDArray[np.float32]
    additional_item: AdditionalItem | None = ...

@dataclass
class AudioGenerateParameters:
    """オーディオサンプル生成に必要なパラメーターをまとめたデータクラス。ジェネレーターのgenerateメソッドに渡される。"""
    start_time: float
    layer: ItemStructure.Audio
    sample_rate: int
    channels: int
    sample_count: int
    args: dict
    input_samples: npt.NDArray[np.float32] | None = ...

class VideoGeneratorBase(SubPluginBase):
    """
    フレームを生成するための基底クラス。サブクラスでオーバーライドして使用することを想定している。
    イベントハンドラーは @event デコレーターで登録する。
    """
    def __init__(self) -> None: ...
    def generate(self, params: VideoGenerateParameters) -> GeneratorWgslReturn | GeneratorFuncReturn | GeneratorTextureReturn | GeneratorBuilderReturn | None:
        """
        フレームを生成するメソッド。サブクラスで必ずオーバーライドする必要がある。

        Args:
            params (VideoGenerateParameters): フレーム生成に必要なパラメーター

        Returns:
            GeneratorWgslReturn | GeneratorFuncReturn | GeneratorTextureReturn | GeneratorBuilderReturn | None:
            生成されたフレームデータ。Noneを返すとその処理はスキップされる。
        """

class AudioGeneratorBase(SubPluginBase):
    """
    オーディオサンプルを生成するための基底クラス。サブクラスでオーバーライドして使用することを想定している。
    イベントハンドラーは @event デコレーターで登録する。
    """
    def __init__(self) -> None: ...
    def generate(self, params: AudioGenerateParameters) -> AudioGeneratorReturn | None:
        """
        オーディオサンプルを生成するメソッド。サブクラスで必ずオーバーライドする必要がある。

        Args:
            params (AudioGenerateParameters): オーディオサンプル生成に必要なパラメーター

        Returns:
            AudioGeneratorReturn | None: 生成されたオーディオサンプル(+任意で追加オーディオアイテム)。Noneを返すとその処理はスキップされる。
        """

class VideoObjectGeneratorBase(VideoGeneratorBase):
    """
    ビデオオブジェクトを生成するための基底クラス。
    """
class VideoEffectGeneratorBase(VideoGeneratorBase):
    """
    ビデオエフェクトを適用してフレームを生成するための基底クラス。
    """
class AudioObjectGeneratorBase(AudioGeneratorBase):
    """
    オーディオオブジェクトを生成するための基底クラス。
    """
class AudioEffectGeneratorBase(AudioGeneratorBase):
    """
    オーディオエフェクトを適用してサンプルを生成するための基底クラス。
    """
