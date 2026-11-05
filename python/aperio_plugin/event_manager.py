from typing import Any, Callable, Literal, cast, overload

from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, ItemStructure
from aperio_plugin.plugin_base.generator_base import *

from .generator_registry import GeneratorNotFoundError, GeneratorRegistry
from .plugin_base import GeneratorBase

type EventCallable[T, U] = Callable[[Callable[[Any, T], U]], Callable[[Any, T], U]]
type AfterMakeFrameHandler = Callable[
    [Any, list[ItemStructure], dict[str, ItemResult]], dict[str, ItemResult] | None
]


@overload
def event(*, type: Literal[GeneratorEvent.New]) -> EventCallable[dict, GeneratorInformation]: ...
@overload
def event(*, type: Literal[GeneratorEvent.RequestStructure]) -> EventCallable[dict, GeneratorInformation]: ...
@overload
def event(
    *, type: Literal[GeneratorEvent.BeforeMakeFrame]
) -> EventCallable[list[ItemStructure], list[ItemStructure] | None]: ...
@overload
def event(
    *, type: Literal[GeneratorEvent.AfterMakeFrame]
) -> Callable[[AfterMakeFrameHandler], AfterMakeFrameHandler]: ...
def event(*, type: GeneratorEvent) -> EventCallable:
    """
    プラグインのイベントハンドラーを登録するデコレーター。

    Args:
        type: ハンドルするイベントの種別 (GeneratorEvent.New / GeneratorEvent.RequestStructure / GeneratorEvent.BeforeMakeFrame / GeneratorEvent.AfterMakeFrame)
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

    def reduce_event[T](self, type: GeneratorEvent, func: Callable[[T, Callable[..., Any]], T], initial: T) -> T:
        """
        全ジェネレーターの、指定イベント種別に対応するハンドラーを登録順に畳み込む。

        Args:
            type: イベント種別
            func: (現在の値, ハンドラー) を受け取り、任意の方法でハンドラーを呼び出して新しい値を返す関数
            initial: 初期値

        Returns:
            すべてのハンドラーを畳み込んだ後の値
        """
        acc = initial
        for generator in self._registry:
            handler = self._find_handler(generator, type)
            if handler is not None:
                acc = func(acc, handler)
        return acc

    @staticmethod
    def _find_handler(generator: GeneratorBase, type: GeneratorEvent) -> Callable[[dict], Any] | None:
        for attr_name in dir(generator):
            method = getattr(generator, attr_name, None)
            if callable(method) and type in getattr(method, "_event_type", []):
                return method
        return None
