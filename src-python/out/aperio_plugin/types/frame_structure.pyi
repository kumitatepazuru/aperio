from dataclasses import dataclass, field
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
    value: float
    suffix: str | None = ...

@dataclass
class IntParam:
    id: str
    title: str
    value: int
    suffix: str | None = ...

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
    suffix: str | None = ...

@dataclass
class Vec2FloatParam:
    id: str
    title: str
    x: float
    y: float
    suffix: str | None = ...

@dataclass
class Vec3IntParam:
    id: str
    title: str
    x: int
    y: int
    z: int
    suffix: str | None = ...

@dataclass
class Vec3FloatParam:
    id: str
    title: str
    x: float
    y: float
    z: float
    suffix: str | None = ...

@dataclass
class Vec4IntParam:
    id: str
    title: str
    x: int
    y: int
    z: int
    w: int
    suffix: str | None = ...

@dataclass
class Vec4FloatParam:
    id: str
    title: str
    x: float
    y: float
    z: float
    w: float
    suffix: str | None = ...

@dataclass
class StringParam:
    id: str
    title: str
    value: str

@dataclass
class ColorParam:
    id: str
    title: str
    r: int
    g: int
    b: int
    a: int
    use_alpha: bool

@dataclass
class ListParam:
    id: str
    title: str
    values: list[str] = field(default_factory=list)
RequestStructureParameter = FloatParam | IntParam | BoolParam | Vec2IntParam | Vec2FloatParam | Vec3IntParam | Vec3FloatParam | Vec4IntParam | Vec4FloatParam | StringParam | ColorParam | ListParam
