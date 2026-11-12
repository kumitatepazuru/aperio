import builtins
from typing import Any, Callable, Concatenate, Literal, cast, overload

from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, ItemStructure
from aperio_plugin.plugin_base.generator_base import *

from .generator_registry import GeneratorNotFoundError, GeneratorRegistry
from .plugin_base import GeneratorBase

# self に続いて P を受け取り R を返すハンドラー
type EventCallable[**P, R] = Callable[
    [Callable[Concatenate[Any, P], R]], Callable[Concatenate[Any, P], R]
]


@overload
def event(*, type: Literal[GeneratorEvent.New]) -> EventCallable[[dict], GeneratorInformation]: ...
@overload
def event(*, type: Literal[GeneratorEvent.RequestStructure]) -> EventCallable[[dict], GeneratorInformation]: ...
@overload
def event(
    *, type: Literal[GeneratorEvent.BeforeMakeFrame]
) -> EventCallable[[list[ItemStructure]], list[ItemStructure] | None]: ...
@overload
def event(
    *, type: Literal[GeneratorEvent.AfterMakeFrame]
) -> EventCallable[[list[ItemStructure], dict[str, ItemResult]], dict[str, ItemResult] | None]: ...
@overload
def event(
    *, type: Literal[GeneratorEvent.ItemMove, GeneratorEvent.ItemScale, GeneratorEvent.ItemRotate]
) -> EventCallable[[list[ItemStructure], list[ItemStructure]], list[ItemStructure] | None]: ...
def event(*, type: GeneratorEvent) -> EventCallable[..., Any]:
    """
    プラグインのイベントハンドラーを登録するデコレーター。

    Args:
        type: ハンドルするイベントの種別
    """

    def decorator(func: Callable) -> Callable:
        f = cast(Any, func)
        if getattr(f, "_event_type", None) is not None:
            f._event_type.append(type)
        else:
            f._event_type = [type]
        return func

    return decorator


class EventDispatcher:
    """
    ジェネレーターの @event デコレーター付きメソッドを、イベント種別から探して呼び出す。
    """

    def __init__(self, registry: GeneratorRegistry) -> None:
        self._registry = registry

    def call_event(self, plugin_name: str, type: GeneratorEvent, params: dict) -> GeneratorInformation:
        """
        指定プラグインの、指定イベント種別に対応するデコレーター付きメソッドを呼び出す。

        Args:
            plugin_name: 対象プラグインの名前
            type: イベント種別
            params: イベントハンドラーに渡すパラメータ辞書

        Returns:
            イベント種別に応じた戻り値
        """
        generator = self._registry.find(plugin_name)
        if generator is None:
            raise GeneratorNotFoundError(plugin_name)

        handler = self._find_handler(generator, type)
        if handler is None:
            raise NotImplementedError(
                f"Plugin '{plugin_name}' has no method decorated with @event(type={type!r})"
            )
        return handler(params)

    def reduce_event[T](self, type: GeneratorEvent, initial: T, *args: Any) -> T:
        """
        全ジェネレーターの、指定イベント種別に対応するハンドラーを登録順に畳み込む。
        各ハンドラーは handler(*args, 現在の値) として呼ばれ、新しい値を返すか、None(変更なし)を返す。

        Args:
            type: イベント種別
            initial: 初期値
            *args: 各ハンドラーに現在の値の前に渡す引数

        Returns:
            すべてのハンドラーを畳み込んだ後の値

        Raises:
            TypeError: ハンドラーが現在の値と異なる型の値を返した場合
        """
        acc = initial
        for generator in self._registry:
            handler = self._find_handler(generator, type)
            if handler is None:
                continue
            result = handler(*args, acc)
            if result is None:
                continue
            if not isinstance(result, builtins.type(acc)):
                raise TypeError(
                    f"{type!r} handler must return {builtins.type(acc).__name__} or None, got {builtins.type(result).__name__}"
                )
            acc = result
        return acc

    @staticmethod
    def _find_handler(generator: GeneratorBase, type: GeneratorEvent) -> Callable[..., Any] | None:
        for attr_name in dir(generator):
            method = getattr(generator, attr_name, None)
            if callable(method) and type in getattr(method, "_event_type", []):
                return method
        return None
