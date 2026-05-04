from aperio.frame_structure import GeneratorEvent as GeneratorEvent, NewGeneratorReturn as NewGeneratorReturn, RequestStructureParameter as RequestStructureParameter
from typing import Callable, Literal, overload

@overload
def event(*, type: Literal[GeneratorEvent.New]) -> Callable[[Callable[..., NewGeneratorReturn]], Callable[..., NewGeneratorReturn]]: ...
@overload
def event(*, type: Literal[GeneratorEvent.RequestStructure]) -> Callable[[Callable[..., list[RequestStructureParameter]]], Callable[..., list[RequestStructureParameter]]]: ...

class EventManager:
    """
    プラグインイベントを統一的に呼び出すミックスイン。
    PluginManager と多重継承して使用することを想定している。
    self.object_plugins / self.effect_plugins は PluginManager から MRO 経由で取得する。
    """
    @overload
    def call_event(self, plugin_name: str, type: Literal[GeneratorEvent.New], params: dict) -> NewGeneratorReturn: ...
    @overload
    def call_event(self, plugin_name: str, type: Literal[GeneratorEvent.RequestStructure], params: dict) -> list[RequestStructureParameter]: ...
