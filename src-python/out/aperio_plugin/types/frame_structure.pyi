from typing import TypedDict

class GenerateStructure(TypedDict):
    """
    エフェクト構造を表す辞書の型定義。
    """
    name: str
    parameters: dict

class LayerStructure(TypedDict):
    """
    レイヤー構造を表す辞書の型定義。
    """
    x: int
    y: int
    scale: float
    rotation: float
    alpha: float
    obj: GenerateStructure
    effects: list[GenerateStructure]
