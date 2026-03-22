from . import SubPluginBase as SubPluginBase
from ..types.frame_structure import RequestStructureParameter as RequestStructureParameter
from dataclasses import dataclass
from gpu_util import PyCompiledFunc as PyCompiledFunc, PyCompiledWgsl as PyCompiledWgsl, PyImageGenerator as PyImageGenerator

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

class GeneratorBase(SubPluginBase):
    """
    フレームを生成するための基底クラス。 サブクラスでオーバーライドして使用することを想定している。
    ジェネレーターは、フレーム生成のためのロジックを実装するクラスで、オブジェクトジェネレーターとフィルタージェネレーターの2種類がある。
    オブジェクトジェネレーターは、前提となる映像データがない状態でフレームを生成するためのもので、フィルタージェネレーターは、前提となる映像データが必要な状態でフレームを生成するためのものである。
    ジェネレーターは、生成時に必要な情報を引数として受け取り、生成されたフレームデータを返却する。
    """
    request_args_struct: list[RequestStructureParameter]
    def __init__(self, generator: PyImageGenerator) -> None:
        """
        フレーム生成プラグインの初期化を行う。必要に応じてサブクラスでオーバーライドする。
        """

class ObjectGeneratorBase(GeneratorBase):
    """
    オブジェクトを生成するための基底クラス。 サブクラスでオーバーライドして使用することを想定している。
    オブジェクトは前提となる映像データがないため、生成時に必要な情報はオブジェクト自体の引数のみになる。
    
    // TODO: より詳細な説明を書く
    """
    def __init__(self, generator: PyImageGenerator) -> None:
        """
        フレーム生成プラグインの初期化を行う。必要に応じてサブクラスでオーバーライドする。
        """
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

class FilterGeneratorBase(GeneratorBase):
    """
    フィルターを適用してフレームを生成するための基底クラス。 サブクラスでオーバーライドして使用することを想定している。
    フィルターは前提となる映像データが必要なため、生成時に元のフレームデータを引数として受け取る。

    // TODO: より詳細な説明を書く
    """
    def __init__(self, generator: PyImageGenerator) -> None:
        """
        フィルター生成プラグインの初期化を行う。必要に応じてサブクラスでオーバーライドする。
        """
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
