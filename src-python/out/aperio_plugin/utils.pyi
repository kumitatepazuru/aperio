import cv2
import numpy as np
from _typeshed import Incomplete
from typing import Literal

class ExpandedUMat:
    """numpyに近い操作を実現するためのcv2.UMatのラッパークラス。"""
    umat: Incomplete
    shape: Incomplete
    def __init__(self, umat: cv2.UMat, shape: tuple[int, int, Literal[1, 3, 4]]) -> None:
        """
        ExpandedUMatの初期化を行う。

        Args:
            umat (cv2.UMat): ラップするcv2.UMatオブジェクト
            shape (tuple[int, int, Literal[1, 3, 4]]): 元のnumpy配列の形状 (height, width, channels)
        """
    @classmethod
    def from_numpy(cls, array: np.ndarray) -> ExpandedUMat:
        """
        numpy配列からExpandedUMatを生成するクラスメソッド。

        Args:
            array (np.ndarray): 変換元のnumpy配列

        Returns:
            ExpandedUMat: 変換されたExpandedUMatオブジェクト
        """
