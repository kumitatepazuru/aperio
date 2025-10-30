import numpy as np
from . import SubPluginBase as SubPluginBase
from ..utils import ExpandedUMat as ExpandedUMat
from typing import Literal

class ObjectGeneratorBase(SubPluginBase):
    """
    オブジェクトを生成するための基底クラス。 サブクラスでオーバーライドして使用することを想定している。
    オブジェクトは前提となる映像データがないため、生成時に必要な情報はオブジェクト自体の引数のみになる。
    """
    def __init__(self) -> None:
        """
        フレーム生成プラグインの初期化を行う。必要に応じてサブクラスでオーバーライドする。
        """
    def generate(self, frame_number: int, obj_args: dict, shape: tuple[int, int, Literal[1, 3, 4]]) -> np.ndarray | ExpandedUMat:
        """
        フレームを生成するメソッド。サブクラスで必ずオーバーライドする必要がある。

        Args:
            frame_number (int): 生成するフレームの番号
            obj_args (dict): オブジェクト生成に必要な引数群
            shape (tuple[int]): 生成するフレームの形状 [height, width, channels]

        Returns:
            np.ndarray | ExpandedUMat: 生成されたフレームデータ
        """

class FilterGeneratorBase(SubPluginBase):
    """
    フィルターを適用してフレームを生成するための基底クラス。 サブクラスでオーバーライドして使用することを想定している。
    フィルターは前提となる映像データが必要なため、生成時に元のフレームデータを引数として受け取る。
    """
    def __init__(self) -> None:
        """
        フィルター生成プラグインの初期化を行う。必要に応じてサブクラスでオーバーライドする。
        """
    def generate(self, frame_number: int, frame: np.ndarray | ExpandedUMat, filter_args: dict) -> np.ndarray | ExpandedUMat:
        """
        フレームを生成するメソッド。サブクラスで必ずオーバーライドする必要がある。

        Args:
            frame_number (int): 生成するフレームの番号
            frame (np.ndarray | ExpandedUMat): 元のフレームデータ
            filter_args (dict): フィルター適用に必要な引数群

        Returns:
            np.ndarray | ExpandedUMat: 生成されたフレームデータ
        """
