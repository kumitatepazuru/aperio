from . import SubPluginBase as SubPluginBase
from aperio.frame_structure import ItemStructure as ItemStructure, NewGeneratorReturn as NewGeneratorReturn, RequestStructureParameter as RequestStructureParameter
from aperio.gpu_util import PyCompiledFunc as PyCompiledFunc, PyCompiledTextureFunc as PyCompiledTextureFunc, PyCompiledWgsl as PyCompiledWgsl
from dataclasses import dataclass

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
    layer: ItemStructure
    args: dict
    width: int
    height: int
    fps: float

class GeneratorBase(SubPluginBase):
    """
    フレームを生成するための基底クラス。サブクラスでオーバーライドして使用することを想定している。
    イベントハンドラーは @event デコレーターで登録する。
    """
    def __init__(self) -> None: ...
    def generate(self, params: GenerateParameters) -> GeneratorWgslReturn | GeneratorFuncReturn | GeneratorTextureReturn | None:
        """
        フレームを生成するメソッド。サブクラスで必ずオーバーライドする必要がある。

        Args:
            params (GenerateParameters): フレーム生成に必要なパラメーター

        Returns:
            GeneratorWgslReturn | GeneratorFuncReturn | GeneratorTextureReturn | None:
            生成されたフレームデータ。Noneを返すとその処理はスキップされる。
        """

class ObjectGeneratorBase(GeneratorBase):
    """
    オブジェクトを生成するための基底クラス。
    on_new / on_request_structure は @event デコレーターでサブクラスに実装する。
    """
class EffectGeneratorBase(GeneratorBase):
    """
    エフェクトを適用してフレームを生成するための基底クラス。
    on_new / on_request_structure は @event デコレーターでサブクラスに実装する。
    """
