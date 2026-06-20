from aperio_plugin.plugin_base.generator_base import *
from aperio.item_structures import GeneratorEvent as GeneratorEvent, GeneratorInformation as GeneratorInformation
from typing import Any, Callable, Literal, overload

type EventCallable[T, U] = Callable[[Callable[[Any, T], U]], Callable[[Any, T], U]]
@overload
def event(*, type: Literal[GeneratorEvent.New]) -> EventCallable[dict, GeneratorInformation]: ...
@overload
def event(*, type: Literal[GeneratorEvent.RequestStructure]) -> EventCallable[dict, GeneratorInformation]: ...

class EventManager:
    """
    プラグインイベントを統一的に呼び出すミックスイン。
    PluginManager と多重継承して使用することを想定している。
    self.video_object_plugins / self.audio_object_plugins / self.video_effect_plugins / self.audio_effect_plugins は PluginManager から MRO 経由で取得する。
    """
    @overload
    def call_event(self, plugin_name: str, type: Literal[GeneratorEvent.New], params: dict) -> GeneratorInformation: ...
    @overload
    def call_event(self, plugin_name: str, type: Literal[GeneratorEvent.RequestStructure], params: dict) -> GeneratorInformation: ...
