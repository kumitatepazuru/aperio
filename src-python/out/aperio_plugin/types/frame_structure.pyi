from typing import Literal, TypedDict

class EffectStructure(TypedDict):
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
    channels: Literal[1, 3, 4]
    obj_base: str
    obj_parameters: dict
    effects: list[EffectStructure]
