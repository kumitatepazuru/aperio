from typing import Any, Callable, Literal, cast, overload

from aperio.item_structures import GeneratorEvent, GeneratorInformation
from aperio_plugin.plugin_base.generator_base import *

from .generator_registry import GeneratorNotFoundError, GeneratorRegistry
from .plugin_base import GeneratorBase

type EventCallable[T, U] = Callable[[Callable[[Any, T], U]], Callable[[Any, T], U]]


@overload
def event(*, type: Literal[GeneratorEvent.New]) -> EventCallable[dict, GeneratorInformation]: ...
@overload
def event(*, type: Literal[GeneratorEvent.RequestStructure]) -> EventCallable[dict, GeneratorInformation]: ...
def event(*, type: GeneratorEvent) -> EventCallable:
    """
    プラグインのイベントハンドラーを登録するデコレーター。

    Args:
        type: ハンドルするイベントの種別 (GeneratorEvent.New / GeneratorEvent.RequestStructure)
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

    @staticmethod
    def _find_handler(generator: GeneratorBase, type: GeneratorEvent) -> Callable[[dict], Any] | None:
        for attr_name in dir(generator):
            method = getattr(generator, attr_name, None)
            if callable(method) and type in getattr(method, "_event_type", []):
                return method
        return None
