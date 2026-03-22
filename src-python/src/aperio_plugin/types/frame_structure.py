from dataclasses import dataclass, field
from typing import Optional, TypedDict, Union


@dataclass
class GenerateStructure(TypedDict):
    """
    エフェクト構造を表す辞書の型定義。
    """

    name: str
    parameters: dict  # パラメータの具体的な型はエフェクトによって異なるため、単にdict型とする


@dataclass
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

@dataclass
class FloatParam:
    id: str
    title: str
    value: float
    suffix: Optional[str] = None


@dataclass
class IntParam:
    id: str
    title: str
    value: int
    suffix: Optional[str] = None


@dataclass
class BoolParam:
    id: str
    title: str
    value: bool


@dataclass
class Vec2IntParam:
    id: str
    title: str
    x: int
    y: int
    suffix: Optional[str] = None


@dataclass
class Vec2FloatParam:
    id: str
    title: str
    x: float
    y: float
    suffix: Optional[str] = None


@dataclass
class Vec3IntParam:
    id: str
    title: str
    x: int
    y: int
    z: int
    suffix: Optional[str] = None


@dataclass
class Vec3FloatParam:
    id: str
    title: str
    x: float
    y: float
    z: float
    suffix: Optional[str] = None


@dataclass
class Vec4IntParam:
    id: str
    title: str
    x: int
    y: int
    z: int
    w: int
    suffix: Optional[str] = None


@dataclass
class Vec4FloatParam:
    id: str
    title: str
    x: float
    y: float
    z: float
    w: float
    suffix: Optional[str] = None


@dataclass
class StringParam:
    id: str
    title: str
    value: str


@dataclass
class ColorParam:
    id: str
    title: str
    r: int  # u8: 0-255
    g: int  # u8: 0-255
    b: int  # u8: 0-255
    a: int  # u8: 0-255
    use_alpha: bool


@dataclass
class ListParam:
    id: str
    title: str
    values: list[str] = field(default_factory=list)


RequestStructureParameter = Union[
    FloatParam,
    IntParam,
    BoolParam,
    Vec2IntParam,
    Vec2FloatParam,
    Vec3IntParam,
    Vec3FloatParam,
    Vec4IntParam,
    Vec4FloatParam,
    StringParam,
    ColorParam,
    ListParam,
]