from typing import TypedDict


class GenerateStructure(TypedDict):
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
    scale: float  # レイヤーのスケール
    rotation: float  # レイヤーの回転角度（度単位）
    alpha: float  # レイヤーの透明度（0.0〜1.0）
    obj: GenerateStructure  # ベースとなるオブジェクトプラグインの情報
    effects: list[GenerateStructure]
