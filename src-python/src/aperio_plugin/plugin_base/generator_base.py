from dataclasses import dataclass

from aperio.frame_structure import ItemStructure, NewGeneratorReturn, RequestStructureParameter
from aperio.gpu_util import PyCompiledFunc, PyCompiledTextureFunc, PyCompiledWgsl

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
class GenerateParameters:
    """フレーム生成に必要なパラメーターをまとめたデータクラス。ジェネレーターのgenerateメソッドに渡される。"""

    frame_number: int
    """生成するフレームの番号"""
    layer: ItemStructure
    """生成するフレームのレイヤー情報"""
    args: dict
    """フレーム生成に必要な引数群"""
    width: int
    """生成元のフレームの幅。オブジェクトの場合はフレームサイズ、エフェクトの場合はオブジェクトサイズが入っている"""
    height: int
    """生成元のフレームの高さ。オブジェクトの場合はフレームサイズ、エフェクトの場合はオブジェクトサイズが入っている"""
    fps: float
    """フレームレート"""


class GeneratorBase(SubPluginBase):
    """
    フレームを生成するための基底クラス。サブクラスでオーバーライドして使用することを想定している。
    イベントハンドラーは @event デコレーターで登録する。
    """

    def __init__(self):
        super().__init__()

    def generate(
        self, params: GenerateParameters
    ) -> GeneratorWgslReturn | GeneratorFuncReturn | GeneratorTextureReturn | None:
        """
        フレームを生成するメソッド。サブクラスで必ずオーバーライドする必要がある。

        Args:
            params (GenerateParameters): フレーム生成に必要なパラメーター

        Returns:
            GeneratorWgslReturn | GeneratorFuncReturn | GeneratorTextureReturn | None:
            生成されたフレームデータ。Noneを返すとその処理はスキップされる。
        """
        raise NotImplementedError("Subclasses must implement this method")


class ObjectGeneratorBase(GeneratorBase):
    """
    オブジェクトを生成するための基底クラス。
    on_new / on_request_structure は @event デコレーターでサブクラスに実装する。
    """

    pass


class EffectGeneratorBase(GeneratorBase):
    """
    エフェクトを適用してフレームを生成するための基底クラス。
    on_new / on_request_structure は @event デコレーターでサブクラスに実装する。
    """

    pass
