from typing import Iterator, Literal, overload

from .plugin_base import GeneratorBase
from .plugin_base.generator_base import (
    AudioEffectGeneratorBase,
    AudioObjectGeneratorBase,
    GeneratorKind,
    VideoEffectGeneratorBase,
    VideoObjectGeneratorBase,
)


class GeneratorNotFoundError(KeyError):
    """指定された名前・種別のジェネレーターが登録されていない場合に送出される。"""

    def __init__(self, name: str, kind: GeneratorKind | None = None) -> None:
        self.name = name
        self.kind = kind
        label = f"{kind.display_name} generator" if kind is not None else "Generator"
        super().__init__(f"{label} '{name}' is not registered")

    def __str__(self) -> str:
        # KeyError の既定の __str__ はメッセージを repr で囲ってしまうため
        return str(self.args[0])


class GeneratorRegistry:
    """
    全種別のジェネレーターを名前で一元管理するレジストリ。
    種別はジェネレーター自身のkind (GeneratorKind) で判別するため、名前は種別をまたいで一意である必要がある。
    """

    def __init__(self) -> None:
        self._generators: dict[str, GeneratorBase] = {}

    def add(self, owner_name: str, generator: GeneratorBase) -> None:
        """
        ジェネレーターを登録する。

        Args:
            owner_name: ジェネレーターを提供するメインプラグインの名前。ジェネレーター名は{owner_name}.で始まる必要がある
            generator: 登録するジェネレーターのインスタンス
        """
        if not isinstance(generator, GeneratorBase) or not isinstance(getattr(generator, "kind", None), GeneratorKind):
            raise TypeError(
                "The generator must be an instance of VideoObjectGeneratorBase, VideoEffectGeneratorBase, "
                "AudioObjectGeneratorBase or AudioEffectGeneratorBase"
            )
        if not generator.name.startswith(owner_name + "."):
            raise ValueError(
                f"Generator name '{generator.name}' should start with '{owner_name}.'. "
                "Please rename the generator or check the main plugin name."
            )
        if generator.name in self._generators:
            raise ValueError(f"Generator '{generator.name}' is already registered")

        self._generators[generator.name] = generator

    def remove_owned_by(self, owner_name: str) -> None:
        """指定されたメインプラグインが登録したジェネレーターをすべて取り除く(初期化に失敗したプラグインの後始末用)。"""
        prefix = owner_name + "."
        for name in [n for n in self._generators if n.startswith(prefix)]:
            del self._generators[name]

    @overload
    def get(self, name: str, kind: Literal[GeneratorKind.VIDEO_OBJECT]) -> VideoObjectGeneratorBase: ...
    @overload
    def get(self, name: str, kind: Literal[GeneratorKind.VIDEO_EFFECT]) -> VideoEffectGeneratorBase: ...
    @overload
    def get(self, name: str, kind: Literal[GeneratorKind.AUDIO_OBJECT]) -> AudioObjectGeneratorBase: ...
    @overload
    def get(self, name: str, kind: Literal[GeneratorKind.AUDIO_EFFECT]) -> AudioEffectGeneratorBase: ...
    def get(self, name: str, kind: GeneratorKind) -> GeneratorBase:
        """
        指定された名前・種別のジェネレーターを返す。

        Raises:
            GeneratorNotFoundError: 未登録、または登録されているが種別が異なる場合
        """
        generator = self._generators.get(name)
        if generator is None or generator.kind is not kind:
            raise GeneratorNotFoundError(name, kind)
        return generator

    def find(self, name: str) -> GeneratorBase | None:
        """種別を問わず、指定された名前のジェネレーターを返す。見つからなければ None。"""
        return self._generators.get(name)

    def of_kind(self, kind: GeneratorKind) -> Iterator[GeneratorBase]:
        """指定された種別のジェネレーターを登録順に列挙する。"""
        return (g for g in self._generators.values() if g.kind is kind)

    def count(self, kind: GeneratorKind) -> int:
        return sum(1 for _ in self.of_kind(kind))

    def display_names(self, kind: GeneratorKind) -> dict[str, str]:
        """UI 向けに、指定された種別の (name → display_name) を返す。is_hidden のものは除外する。"""
        return {g.name: g.display_name for g in self.of_kind(kind) if not g.is_hidden}

    def __len__(self) -> int:
        return len(self._generators)
