from .audio import AudioManager
from .config_manager import ConfigManager
from .store import StoreManager
from _typeshed import Incomplete
from typing import final

@final
class PyManagers:
    @property
    def audio_manager(self, /) -> AudioManager: ...
    @property
    def config_manager(self, /) -> ConfigManager: ...
    @property
    def store_manager(self, /) -> StoreManager: ...

def __getattr__(name: str) -> Incomplete: ...
