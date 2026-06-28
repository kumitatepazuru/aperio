from dataclasses import dataclass
from aperio.item_structures import ItemStructure
from aperio.gpu_util import PyCompiledFunc, PyCompiledTextureFunc, PyCompiledWgsl, PyImageGenerateBuilder
import numpy as np
import numpy.typing as npt

from . import SubPluginBase


@dataclass
class GeneratorWgslReturn:
    compiled: PyCompiledWgsl
    params: bytes
    output_width: int
    output_height: int


@dataclass
class GeneratorFuncReturn:
    compiled: PyCompiledFunc
    params: object
    output_width: int
    output_height: int


@dataclass
class GeneratorTextureReturn:
    compiled: PyCompiledTextureFunc
    params: object
    output_width: int
    output_height: int


@dataclass
class GeneratorBuilderReturn:
    builder: PyImageGenerateBuilder
    output_width: int
    output_height: int


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

class VideoGeneratorBase(SubPluginBase):
    """
    フレームを生成するための基底クラス。サブクラスでオーバーライドして使用することを想定している。
    イベントハンドラーは @event デコレーターで登録する。
    """

    def __init__(self):
        super().__init__()

    def generate(
        self, params: VideoGenerateParameters
    ) -> GeneratorWgslReturn | GeneratorFuncReturn | GeneratorTextureReturn | GeneratorBuilderReturn | None:
        """
        フレームを生成するメソッド。サブクラスで必ずオーバーライドする必要がある。

        Args:
            params (VideoGenerateParameters): フレーム生成に必要なパラメーター

        Returns:
            GeneratorWgslReturn | GeneratorFuncReturn | GeneratorTextureReturn | GeneratorBuilderReturn | None:
            生成されたフレームデータ。Noneを返すとその処理はスキップされる。
        """
        raise NotImplementedError("Subclasses must implement this method")
    
class AudioGeneratorBase(SubPluginBase):
    """
    オーディオサンプルを生成するための基底クラス。サブクラスでオーバーライドして使用することを想定している。
    イベントハンドラーは @event デコレーターで登録する。
    """

    def __init__(self):
        super().__init__()

    def generate(self, params: AudioGenerateParameters) -> npt.NDArray[np.float32] | None:
        """
        オーディオサンプルを生成するメソッド。サブクラスで必ずオーバーライドする必要がある。

        Args:
            params (AudioGenerateParameters): オーディオサンプル生成に必要なパラメーター

        Returns:
            npt.NDArray[np.float32] | None: 生成されたオーディオサンプルの2次元配列。Noneを返すとその処理はスキップされる。
        """
        raise NotImplementedError("Subclasses must implement this method")


class VideoObjectGeneratorBase(VideoGeneratorBase):
    """
    ビデオオブジェクトを生成するための基底クラス。
    """

    pass


class VideoEffectGeneratorBase(VideoGeneratorBase):
    """
    ビデオエフェクトを適用してフレームを生成するための基底クラス。
    """

    pass


class AudioObjectGeneratorBase(AudioGeneratorBase):
    """
    オーディオオブジェクトを生成するための基底クラス。
    """

    pass

class AudioEffectGeneratorBase(AudioGeneratorBase):
    """
    オーディオエフェクトを適用してサンプルを生成するための基底クラス。
    """

    pass