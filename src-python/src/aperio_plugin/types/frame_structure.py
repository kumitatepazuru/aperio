from typing import Literal, TypedDict


class EffectStructure(TypedDict):
    """
    エフェクト構造を表す辞書の型定義。
    """

    name: str
    parameters: dict  # パラメータの具体的な型はエフェクトによって異なるため、単にdict型とする


class LayerStructure(TypedDict):
    """
    レイヤー構造を表す辞書の型定義。
    """

    x: int  # レイヤーの左上隅のX座標
    y: int  # レイヤーの左上隅のY座標
    channels: Literal[1, 3, 4]  # 1: グレースケール, 3: RGB, 4: RGBA
    obj_base: str  # ベースとなるオブジェクトプラグインの名前
    obj_parameters: dict  # オブジェクトプラグインのパラメータ
    effects: list[EffectStructure]
