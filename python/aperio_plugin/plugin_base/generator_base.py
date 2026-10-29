from dataclasses import dataclass
from enum import Enum
from typing import ClassVar
from aperio.item_structures import AdditionalItem, ItemResult, ItemStructure
from aperio.gpu_util import PyCompiledFunc, PyCompiledTextureFunc, PyCompiledShader, PyImageGenerateBuilder
import numpy as np
import numpy.typing as npt

from . import GeneratorBase


class GeneratorKind(Enum):
    """ジェネレーターの種別。各ジェネレーター基底クラスの ``kind`` に設定されている。"""

    VIDEO_OBJECT = "video_object"
    VIDEO_EFFECT = "video_effect"
    AUDIO_OBJECT = "audio_object"
    AUDIO_EFFECT = "audio_effect"

    @property
    def display_name(self) -> str:
        """ログ表示用の名前 (例: ``Video Object``)"""
        return self.value.replace("_", " ").title()


@dataclass
class GeneratorShaderReturn:
    compiled: PyCompiledShader
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
    """生成するフレームの番号"""
    layer: "ItemStructure.Video"
    """生成するフレームのレイヤー情報"""
    args: dict
    """フレーム生成に必要な引数群"""
    width: int
    """生成元のフレームの幅。オブジェクトの場合はフレームサイズ、エフェクトの場合はオブジェクトサイズが入っている"""
    height: int
    """生成元のフレームの高さ。オブジェクトの場合はフレームサイズ、エフェクトの場合はオブジェクトサイズが入っている"""
    structure_id: str = ""
    """この呼び出しが対応する`GenerateStructure.id`(`layer.object["id"]`または対応するeffectの`["id"]`)。"""

@dataclass
class AudioGeneratorReturn:
    """オーディオの generate() の戻り値。additional_item は video 側の
    ItemResult.additional_item と対称な仕組みで、自分と同じ時間窓に
    加算ミックスする追加のオーディオアイテムを指定できる(behind は無視される。
    音声は加算合成なので順序に意味が無いため)。"""

    samples: "npt.NDArray[np.float32]"
    additional_item: "AdditionalItem | None" = None


@dataclass
class AudioGenerateParameters:
    """オーディオサンプル生成に必要なパラメーターをまとめたデータクラス。ジェネレーターのgenerateメソッドに渡される。"""

    start_time: float
    """生成を開始する時間（秒）。フレーム番号にfpsをかけたもの"""
    layer: "ItemStructure.Audio"
    """生成するサンプルのレイヤー情報"""
    sample_rate: int
    """サンプルレート"""
    channels: int
    """チャンネル数"""
    sample_count: int
    """生成するサンプル数"""
    args: dict
    """サンプル生成に必要な引数群"""
    input_samples: "npt.NDArray[np.float32] | None" = None
    """エフェクト適用時に渡される前段のサンプル。オブジェクトプラグインにはNoneが渡される"""

class VideoGeneratorBase(GeneratorBase):
    """
    フレームを生成するための基底クラス。サブクラスでオーバーライドして使用することを想定している。
    イベントハンドラーは @event デコレーターで登録する。
    """

    def __init__(self):
        super().__init__()

    def generate(
        self, params: VideoGenerateParameters
    ) -> GeneratorShaderReturn | GeneratorFuncReturn | GeneratorTextureReturn | GeneratorBuilderReturn | ItemResult | None:
        """
        フレームを生成するメソッド。サブクラスで必ずオーバーライドする必要がある。

        Args:
            params (VideoGenerateParameters): フレーム生成に必要なパラメーター

        Returns:
            GeneratorShaderReturn | GeneratorFuncReturn | GeneratorTextureReturn | GeneratorBuilderReturn | ItemResult | None:
            生成されたフレームデータ。Noneを返すとその処理はスキップされる。
        """
        raise NotImplementedError("Subclasses must implement this method")
    
class AudioGeneratorBase(GeneratorBase):
    """
    オーディオサンプルを生成するための基底クラス。サブクラスでオーバーライドして使用することを想定している。
    イベントハンドラーは @event デコレーターで登録する。
    """

    def __init__(self):
        super().__init__()

    def generate(self, params: AudioGenerateParameters) -> AudioGeneratorReturn | None:
        """
        オーディオサンプルを生成するメソッド。サブクラスで必ずオーバーライドする必要がある。

        Args:
            params (AudioGenerateParameters): オーディオサンプル生成に必要なパラメーター

        Returns:
            AudioGeneratorReturn | None: 生成されたオーディオサンプル(+任意で追加オーディオアイテム)。Noneを返すとその処理はスキップされる。
        """
        raise NotImplementedError("Subclasses must implement this method")


class VideoObjectGeneratorBase(VideoGeneratorBase):
    """
    ビデオオブジェクトを生成するための基底クラス。
    """

    kind: ClassVar[GeneratorKind] = GeneratorKind.VIDEO_OBJECT


class VideoEffectGeneratorBase(VideoGeneratorBase):
    """
    ビデオエフェクトを適用してフレームを生成するための基底クラス。
    """

    kind: ClassVar[GeneratorKind] = GeneratorKind.VIDEO_EFFECT


class AudioObjectGeneratorBase(AudioGeneratorBase):
    """
    オーディオオブジェクトを生成するための基底クラス。
    """

    kind: ClassVar[GeneratorKind] = GeneratorKind.AUDIO_OBJECT

class AudioEffectGeneratorBase(AudioGeneratorBase):
    """
    オーディオエフェクトを適用してサンプルを生成するための基底クラス。
    """

    kind: ClassVar[GeneratorKind] = GeneratorKind.AUDIO_EFFECT