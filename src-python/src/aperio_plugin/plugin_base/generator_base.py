from dataclasses import dataclass
from typing import Literal

from gpu_util import PyCompiledFunc, PyCompiledWgsl, PyImageGenerator
import numpy as np

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

class ObjectGeneratorBase(SubPluginBase):
    """
    オブジェクトを生成するための基底クラス。 サブクラスでオーバーライドして使用することを想定している。
    オブジェクトは前提となる映像データがないため、生成時に必要な情報はオブジェクト自体の引数のみになる。
    
    // TODO: より詳細な説明を書く
    """

    def __init__(self, generator: PyImageGenerator):
        """
        フレーム生成プラグインの初期化を行う。必要に応じてサブクラスでオーバーライドする。
        """
        super().__init__()

    def generate(self, frame_number: int, obj_args: dict, width: int, height: int) -> GeneratorWgslReturn | GeneratorFuncReturn:
        """
        フレームを生成するメソッド。サブクラスで必ずオーバーライドする必要がある。

        Args:
            frame_number (int): 生成するフレームの番号
            obj_args (dict): オブジェクト生成に必要な引数群
            width (int): 生成するフレームの幅
            height (int): 生成するフレームの高さ

        Returns:
            GeneratorWgslReturn | GeneratorFuncReturn: 生成されたフレームデータ
        """
        raise NotImplementedError("Subclasses must implement this method")


class FilterGeneratorBase(SubPluginBase):
    """
    フィルターを適用してフレームを生成するための基底クラス。 サブクラスでオーバーライドして使用することを想定している。
    フィルターは前提となる映像データが必要なため、生成時に元のフレームデータを引数として受け取る。

    // TODO: より詳細な説明を書く
    """

    def __init__(self, generator: PyImageGenerator):
        """
        フィルター生成プラグインの初期化を行う。必要に応じてサブクラスでオーバーライドする。
        """
        super().__init__()

    def generate(self, frame_number: int, filter_args: dict, width: int, height: int) -> GeneratorWgslReturn | GeneratorFuncReturn:
        """
        フレームを生成するメソッド。サブクラスで必ずオーバーライドする必要がある。

        Args:
            frame_number (int): 生成するフレームの番号
            filter_args (dict): フィルター適用に必要な引数群
            width (int): 生成するフレームの幅
            height (int): 生成するフレームの高さ

        Returns:
            GeneratorWgslReturn | GeneratorFuncReturn: 生成されたフレームデータ
        """
        raise NotImplementedError("Subclasses must implement this method")
