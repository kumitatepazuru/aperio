from typing import Any, Callable, Literal, cast, overload

from aperio.item_structures import GeneratorEvent, GeneratorInformation
from aperio_plugin.plugin_base.generator_base import *

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


class EventManager:
    """
    プラグインイベントを統一的に呼び出すミックスイン。
    PluginManager と多重継承して使用することを想定している。
    self.video_object_plugins / self.audio_object_plugins / self.video_effect_plugins / self.audio_effect_plugins は PluginManager から MRO 経由で取得する。
    """

    @overload
    def call_event(
        self,
        plugin_name: str,
        type: Literal[GeneratorEvent.New],
        params: dict,
    ) -> GeneratorInformation: ...
    @overload
    def call_event(
        self,
        plugin_name: str,
        type: Literal[GeneratorEvent.RequestStructure],
        params: dict,
    ) -> GeneratorInformation: ...
    def call_event(self, plugin_name: str, type: GeneratorEvent, params: dict):
        """
        指定プラグインの、指定イベント種別に対応するデコレーター付きメソッドを呼び出す。

        Args:
            plugin_name: 対象プラグインの名前
            type: イベント種別
            params: イベントハンドラーに渡すパラメータ辞書

        Returns:
            イベント種別に応じた戻り値
        """
        plugin = self.video_object_plugins.get(plugin_name) or self.audio_object_plugins.get(plugin_name) or self.video_effect_plugins.get(plugin_name) or self.audio_effect_plugins.get(plugin_name) # type: ignore
        if plugin is None:
            raise ValueError(
                f"Plugin '{plugin_name}' is not registered as object or effect plugin"
            )

        for attr_name in dir(plugin):
            method = getattr(plugin, attr_name, None)
            if callable(method) and type in getattr(method, "_event_type", []):
                return method(params)

        raise NotImplementedError(
            f"Plugin '{plugin_name}' has no method decorated with @event(type={type!r})"
        )
