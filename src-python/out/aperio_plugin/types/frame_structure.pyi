from dataclasses import dataclass, field as field
from typing import TypedDict

@dataclass
class GenerateStructure(TypedDict):
    """
    エフェクト構造を表す辞書の型定義。
    """
    name: str
    parameters: dict

@dataclass
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

@dataclass
class FloatParam:
    id: str
    title: str
    default_value: float
    suffix: str | None = ...

@dataclass
class IntParam:
    id: str
    title: str
    default_value: int
    suffix: str | None = ...

@dataclass
class BoolParam:
    id: str
    title: str
    default_value: bool

@dataclass
class Vec2IntParam:
    id: str
    title: str
    default_value: tuple[int, int]
    suffix: str | None = ...

@dataclass
class Vec2FloatParam:
    id: str
    title: str
    default_value: tuple[float, float]
    suffix: str | None = ...

@dataclass
class Vec3IntParam:
    id: str
    title: str
    default_value: tuple[int, int, int]
    suffix: str | None = ...

@dataclass
class Vec3FloatParam:
    id: str
    title: str
    default_value: tuple[float, float, float]
    suffix: str | None = ...

@dataclass
class Vec4IntParam:
    id: str
    title: str
    default_value: tuple[int, int, int, int]
    suffix: str | None = ...

@dataclass
class Vec4FloatParam:
    id: str
    title: str
    default_value: tuple[float, float, float, float]
    suffix: str | None = ...

@dataclass
class StringParam:
    id: str
    title: str
    default_value: str

@dataclass
class ColorParam:
    id: str
    title: str
    default_value: tuple[float, float, float, float]
    use_alpha: bool

@dataclass
class ListParam:
    id: str
    title: str
    values: dict[str, str]
    default_key: str
RequestStructureParameter = FloatParam | IntParam | BoolParam | Vec2IntParam | Vec2FloatParam | Vec3IntParam | Vec3FloatParam | Vec4IntParam | Vec4FloatParam | StringParam | ColorParam | ListParam
