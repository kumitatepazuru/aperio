from typing import Callable, Literal, overload

from aperio.frame_structure import GeneratorEvent, NewGeneratorReturn, RequestStructureParameter


@overload
def event(
    *, type: Literal[GeneratorEvent.New]
) -> Callable[[Callable[..., NewGeneratorReturn]], Callable[..., NewGeneratorReturn]]: ...


@overload
def event(
    *, type: Literal[GeneratorEvent.RequestStructure]
) -> Callable[
    [Callable[..., list[RequestStructureParameter]]],
    Callable[..., list[RequestStructureParameter]],
]: ...


def event(*, type: GeneratorEvent):
    """
    プラグインのイベントハンドラーを登録するデコレーター。

    Args:
        type: ハンドルするイベントの種別 (GeneratorEvent.New / GeneratorEvent.RequestStructure)
    """

    def decorator(func: Callable) -> Callable:
        func._event_type = type
        return func

    return decorator


class EventManager:
    """
    プラグインイベントを統一的に呼び出すミックスイン。
    PluginManager と多重継承して使用することを想定している。
    self.object_plugins / self.effect_plugins は PluginManager から MRO 経由で取得する。
    """

    @overload
    def call_event(
        self,
        plugin_name: str,
        type: Literal[GeneratorEvent.New],
        params: dict,
    ) -> NewGeneratorReturn: ...

    @overload
    def call_event(
        self,
        plugin_name: str,
        type: Literal[GeneratorEvent.RequestStructure],
        params: dict,
    ) -> list[RequestStructureParameter]: ...

    def call_event(self, plugin_name: str, type: GeneratorEvent, params: dict):
        """
        指定プラグインの、指定イベント種別に対応するデコレーター付きメソッドを呼び出す。

        Args:
            plugin_name: 対象プラグインの名前
            type: イベント種別
            params: イベントハンドラーに渡すパラメータ辞書

        Returns:
            イベント種別に応じた戻り値（NewGeneratorReturn または list[RequestStructureParameter]）
        """
        plugin = self.object_plugins.get(plugin_name) or self.effect_plugins.get(plugin_name)  # type: ignore[attr-defined]
        if plugin is None:
            raise ValueError(
                f"Plugin '{plugin_name}' is not registered as object or effect plugin"
            )

        for attr_name in dir(plugin):
            method = getattr(plugin, attr_name, None)
            if callable(method) and getattr(method, "_event_type", None) == type:
                return method(params)

        raise NotImplementedError(
            f"Plugin '{plugin_name}' has no method decorated with @event(type={type!r})"
        )
